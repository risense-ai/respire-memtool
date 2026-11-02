/* ============================================================
   memTools AI / 知识库窗口
   - 聊天：invoke ai_send → ai://event 流式回传（含思考中动态）
   - 入库：invoke kb_submit → kb://task 进度（含已用时）
   ============================================================ */

const $ = (id) => document.getElementById(id);

function tauriApi() {
  const api = window.__TAURI__;
  if (!api || !api.core || typeof api.core.invoke !== "function") return null;
  return api;
}

async function call(cmd, args) {
  const api = tauriApi();
  if (!api) throw new Error("请在 memTools 中打开");
  return api.core.invoke(cmd, args);
}

async function listenEvent(name, handler) {
  const api = tauriApi();
  if (!api || !api.event || typeof api.event.listen !== "function") return null;
  try {
    return await api.event.listen(name, (event) => {
      try {
        handler(event.payload);
      } catch (_) {
        /* 单条渲染失败不打断后续事件 */
      }
    });
  } catch (_) {
    return null;
  }
}

function messageOf(err) {
  const raw = typeof err === "string" ? err : (err && err.message) || String(err || "");
  return String(raw).replace(/^Error:\s*/i, "").trim() || "操作失败";
}

const state = {
  status: null,
  agents: [],
  tasks: new Map(),
  busy: false,
  assistant: null,
  lastTool: "",
  chatStartedAt: 0,
};

/* ---------------- 状态条 ---------------- */

function setChip(text, kind) {
  const el = $("status-chip");
  if (!el) return;
  el.textContent = text;
  el.className = "chip" + (kind ? ` ${kind}` : "");
  el.title = text;
}

function renderStatus(status) {
  if (!status) return;
  state.status = status;
  const root = (status.root || "").trim();
  const pathEl = $("root-path");
  if (pathEl) {
    if (!root) {
      pathEl.textContent = "未设置（点右上角「知识库目录」选择）";
      pathEl.className = "root-path err";
    } else {
      pathEl.textContent = root;
      pathEl.className = "root-path" + (status.root_ok ? " ok" : " err");
      pathEl.title = root;
    }
  }
  if (!status.enabled) {
    setChip("知识库已关闭（config.toml）", "warn");
  } else if (!status.root_ok) {
    setChip("知识库未就绪：先选目录", "warn");
  } else {
    const agent = status.agent_kind || "agent";
    const model = status.model ? ` · ${status.model}` : "";
    setChip(`就绪 · ${agent}${model}`, "ok");
  }
  const sw = $("context-menu");
  if (sw) {
    sw.checked = !!status.context_menu;
    sw.disabled = !status.platform_supported;
  }
  const sel = $("agent-select");
  if (sel && status.agent_kind) sel.value = status.agent_kind;
  const modelInput = $("agent-model");
  if (modelInput && document.activeElement !== modelInput) {
    modelInput.value = status.model || "";
  }
  updateAgentHint();
}

function updateAgentHint() {
  const hint = $("agent-hint");
  if (!hint) return;
  const kind = state.status?.agent_kind || "";
  const found = state.agents.find((a) => a.kind === kind);
  if (!kind) {
    hint.textContent = "选一个本机已安装的 agent，聊天与入库都用它。";
    return;
  }
  if (found && found.ok) {
    hint.textContent = `${kind} 可用：${found.bin || "自动探测"} ${found.version || ""}`.trim();
  } else if (found) {
    hint.textContent = `${kind} 未检测到：${found.message || "请确认已安装并登录"}`;
  } else {
    hint.textContent = `已绑定 ${kind}；点「绑定」可重新探测。`;
  }
}

async function loadAgents() {
  try {
    const list = await call("agent_list");
    state.agents = Array.isArray(list) ? list : [];
    const sel = $("agent-select");
    if (!sel) return;
    const current = state.status?.agent_kind || "";
    sel.innerHTML = "";
    for (const a of state.agents) {
      const opt = document.createElement("option");
      opt.value = a.kind;
      opt.textContent = a.ok ? `${a.kind} · ${a.version}` : `${a.kind} · 未检测到`;
      opt.disabled = !a.ok && a.kind !== current;
      sel.append(opt);
    }
    if (current && !state.agents.some((a) => a.kind === current)) {
      const opt = document.createElement("option");
      opt.value = current;
      opt.textContent = `${current} · 未检测到`;
      sel.append(opt);
    }
    if (current) sel.value = current;
    updateAgentHint();
  } catch (err) {
    const hint = $("agent-hint");
    if (hint) hint.textContent = "读取 agent 列表失败：" + messageOf(err);
  }
}

async function applyAgent() {
  const sel = $("agent-select");
  if (!sel || !sel.value) return;
  const kind = sel.value;
  const prev = state.status?.agent_kind || "";
  // 切换 agent 时清空模型：opencode 是 provider/model，qoder 是 auto/efficient 档位，不能混用
  const switched = kind !== prev;
  const model = switched ? "" : ($("agent-model")?.value || "").trim();
  const known = state.agents.find((a) => a.kind === kind);
  const bin = known?.bin || "";
  try {
    const status = await call("kb_set_agent", { kind, bin, model });
    renderStatus(status);
    const label = `已绑定 ${kind}${status.model ? " · " + status.model : "（默认模型）"}`;
    setChip(label, "ok");
    appendMessage("tool", `AI 智能体已切换为 ${label}`);
  } catch (err) {
    appendMessage("error", "绑定失败：" + messageOf(err));
  }
}

/* ---------------- 入库任务 ---------------- */

function parseStamp(text) {
  if (!text) return 0;
  const t = Date.parse(String(text).replace(" ", "T"));
  return Number.isFinite(t) ? t : 0;
}

function elapsedText(startedAt) {
  const start = parseStamp(startedAt);
  if (!start) return "";
  const secs = Math.max(0, Math.floor((Date.now() - start) / 1000));
  if (secs < 60) return `${secs} 秒`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m} 分 ${s} 秒`;
}

function taskEl(task) {
  const el = document.createElement("div");
  el.className = "task " + (task.status || "");
  el.dataset.id = task.id;
  el.dataset.started = task.started_at || "";

  const head = document.createElement("div");
  head.className = "t-head";
  const title = document.createElement("span");
  title.className = "t-title";
  title.textContent = task.title || task.kind || "任务";
  const badge = document.createElement("span");
  badge.className = "t-stage";
  head.append(title, badge);

  const running = task.status === "running" || task.status === "queued";
  if (running) {
    badge.innerHTML =
      '<span class="dots"><i></i><i></i><i></i></span><span class="t-elapsed"></span>';
  } else {
    badge.textContent = task.status === "done" ? "完成" : task.status === "failed" ? "失败" : task.stage || "";
  }

  const stage = document.createElement("div");
  stage.className = "t-stage-line";
  stage.textContent = running
    ? task.stage || "处理中"
    : task.status === "done"
      ? "已写入知识库"
      : task.stage || "失败";

  const sub = document.createElement("div");
  sub.className = "t-sub";
  sub.textContent = [task.md_path || task.item_dir, task.message].filter(Boolean).join("\n");

  el.append(head, stage, sub);

  if (task.memory_id) {
    const mem = document.createElement("div");
    mem.className = "t-mem";
    mem.textContent = "记忆 " + task.memory_id;
    el.append(mem);
  }
  if (running) {
    el.classList.add("is-running");
    const elapsed = el.querySelector(".t-elapsed");
    if (elapsed) elapsed.textContent = elapsedText(task.started_at);
  }
  return el;
}

function upsertTask(task) {
  if (!task || !task.id) return;
  state.tasks.set(task.id, task);
  const list = $("tasks");
  if (!list) return;
  const existing = list.querySelector(`[data-id="${cssEscape(task.id)}"]`);
  const el = taskEl(task);
  if (existing) existing.replaceWith(el);
  else list.prepend(el);
}

/** 每秒刷新运行中任务的已用时（不改状态，只改文本） */
function tickTasks() {
  const list = $("tasks");
  if (!list) return;
  list.querySelectorAll(".task.is-running").forEach((el) => {
    const started = el.dataset.started || state.tasks.get(el.dataset.id)?.started_at;
    const span = el.querySelector(".t-elapsed");
    const text = elapsedText(started);
    if (span && text) span.textContent = `已用 ${text}`;
  });
}

function cssEscape(value) {
  return String(value).replace(/[^a-zA-Z0-9_-]/g, (c) => `\\${c}`);
}

/* ---------------- 聊天 ---------------- */

function clearEmpty() {
  const empty = $("chat-empty");
  if (empty) empty.remove();
}

function scrollBottom() {
  const box = $("messages");
  if (box) box.scrollTop = box.scrollHeight;
}

function appendMessage(role, text) {
  clearEmpty();
  const wrap = document.createElement("div");
  wrap.className = `msg ${role}`;
  const who = document.createElement("div");
  who.className = "who";
  who.textContent = role === "user" ? "你" : role === "tool" ? "工具" : "AI";
  const body = document.createElement("div");
  body.className = "body";
  body.textContent = text;
  wrap.append(who, body);
  $("messages").append(wrap);
  scrollBottom();
  return body;
}

/** 新建一个 AI 气泡：先显示「思考中…」，首个文本到达后替换 */
function appendAssistant() {
  clearEmpty();
  const wrap = document.createElement("div");
  wrap.className = "msg ai";

  const who = document.createElement("div");
  who.className = "who";
  who.textContent = "AI";

  const body = document.createElement("div");
  body.className = "body";

  const thinking = document.createElement("div");
  thinking.className = "thinking";
  thinking.innerHTML =
    '<span class="thinking-text">思考中</span><span class="dots"><i></i><i></i><i></i></span><span class="thinking-time"></span>';

  const activity = document.createElement("div");
  activity.className = "activity hidden";

  const textEl = document.createElement("div");
  textEl.className = "answer hidden";

  body.append(thinking, activity, textEl);
  wrap.append(who, body);
  $("messages").append(wrap);
  scrollBottom();

  const started = Date.now();
  const timer = window.setInterval(() => {
    const el = thinking.querySelector(".thinking-time");
    if (el) el.textContent = `已 ${Math.floor((Date.now() - started) / 1000)} 秒`;
  }, 1000);

  return { wrap, body, thinking, activity, textEl, timer, started };
}

function stopThinking(a) {
  if (!a) return;
  if (a.timer) window.clearInterval(a.timer);
  a.timer = 0;
  if (a.thinking) a.thinking.classList.add("hidden");
}

function setActivity(a, text) {
  if (!a || !text) return;
  a.activity.classList.remove("hidden");
  a.activity.textContent = text;
  scrollBottom();
}

function setBusy(busy) {
  state.busy = !!busy;
  $("btn-send").disabled = busy;
  $("btn-stop").disabled = !busy;
}

async function sendChat() {
  const input = $("chat-input");
  const text = (input.value || "").trim();
  if (!text || state.busy) return;
  input.value = "";
  appendMessage("user", text);
  state.assistant = appendAssistant();
  state.lastTool = "";
  setBusy(true);
  try {
    await call("ai_send", { text });
  } catch (err) {
    stopThinking(state.assistant);
    setBusy(false);
    appendMessage("error", messageOf(err));
  }
}

async function stopChat() {
  try {
    await call("ai_abort");
  } catch (_) {
    /* 停止失败不影响界面恢复 */
  }
}

function onAiEvent(payload) {
  if (!payload || !payload.kind) return;
  const current = state.assistant;
  switch (payload.kind) {
    case "start":
      if (!current) state.assistant = appendAssistant();
      setBusy(true);
      break;
    case "text": {
      if (!state.assistant) state.assistant = appendAssistant();
      const a = state.assistant;
      stopThinking(a);
      a.textEl.classList.remove("hidden");
      a.textEl.textContent += payload.text || "";
      scrollBottom();
      break;
    }
    case "tool":
      if (payload.text && payload.text !== state.lastTool) {
        state.lastTool = payload.text;
        if (!state.assistant) state.assistant = appendAssistant();
        setActivity(state.assistant, payload.text);
      }
      break;
    case "error":
      stopThinking(state.assistant);
      appendMessage("error", payload.text || "出错了");
      break;
    case "done":
      stopThinking(state.assistant);
      setBusy(false);
      state.assistant = null;
      break;
    case "reset": {
      const box = $("messages");
      if (box) box.innerHTML = "";
      stopThinking(state.assistant);
      setBusy(false);
      state.assistant = null;
      break;
    }
    default:
      break;
  }
}

/* ---------------- 入库操作 ---------------- */

async function submitIngest(req) {
  try {
    const task = await call("kb_submit", { req });
    if (task) upsertTask(task);
    return task;
  } catch (err) {
    appendMessage("error", "入库失败：" + messageOf(err));
    return null;
  }
}

async function drainPending() {
  try {
    const list = await call("kb_take_pending");
    if (Array.isArray(list) && list.length) {
      for (const item of list) {
        await submitIngest(item);
      }
    }
  } catch (_) {
    /* 无待处理项 */
  }
}

async function pickRoot() {
  try {
    const picked = await call("kb_pick_root");
    if (!picked) return;
    const status = await call("kb_set_root", { root: picked });
    renderStatus(status);
    await drainPending();
  } catch (err) {
    setChip(messageOf(err), "err");
  }
}

async function addFolder() {
  try {
    const picked = await call("kb_pick_folder");
    if (!picked) return;
    await submitIngest({ kind: "folder", value: picked, title: "", source: "" });
  } catch (err) {
    appendMessage("error", "选择文件夹失败：" + messageOf(err));
  }
}

async function addPath() {
  const input = $("path-input");
  const value = (input.value || "").trim();
  if (!value) return;
  input.value = "";
  await submitIngest({ kind: "", value, title: "", source: "" });
}

async function addUrl() {
  const input = $("url-input");
  const value = (input.value || "").trim();
  if (!value) return;
  input.value = "";
  await submitIngest({ kind: "url", value, title: "", source: "" });
}

async function toggleContextMenu(enabled) {
  try {
    const status = await call("kb_set_context_menu", { enabled });
    renderStatus(status);
    appendMessage(
      "tool",
      enabled ? "已注册资源管理器右键菜单（Win11 在「显示更多选项」里）" : "已移除右键菜单"
    );
  } catch (err) {
    $("context-menu").checked = !enabled;
    appendMessage("error", "右键菜单设置失败：" + messageOf(err));
  }
}

/* ---------------- 拖放 ---------------- */

function setupDragDrop() {
  const api = tauriApi();
  if (!api || !api.webview || typeof api.webview.getCurrentWebview !== "function") return;
  try {
    const webview = api.webview.getCurrentWebview();
    if (!webview || typeof webview.onDragDropEvent !== "function") return;
    webview.onDragDropEvent((event) => {
      const type = event?.payload?.type;
      if (type === "enter" || type === "over") {
        document.body.classList.add("dragging");
        return;
      }
      document.body.classList.remove("dragging");
      if (type !== "drop") return;
      const paths = event?.payload?.paths || [];
      for (const path of paths) {
        void submitIngest({ kind: "", value: path, title: "", source: "" });
      }
    });
  } catch (_) {
    /* 浏览器预览无拖放路径 */
  }
}

/* ---------------- 启动 ---------------- */

async function init() {
  $("btn-pick-root")?.addEventListener("click", () => void pickRoot());
  $("btn-add-folder")?.addEventListener("click", () => void addFolder());
  $("btn-add-path")?.addEventListener("click", () => void addPath());
  $("btn-add-url")?.addEventListener("click", () => void addUrl());
  $("btn-send")?.addEventListener("click", () => void sendChat());
  $("btn-stop")?.addEventListener("click", () => void stopChat());
  $("btn-new-chat")?.addEventListener("click", async () => {
    try {
      await call("ai_reset");
    } catch (_) {
      /* 忽略 */
    }
  });
  $("context-menu")?.addEventListener("change", (e) => void toggleContextMenu(!!e.target.checked));
  $("btn-agent-save")?.addEventListener("click", () => void applyAgent());
  $("agent-select")?.addEventListener("change", () => void applyAgent());
  $("agent-model")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void applyAgent();
    }
  });

  $("path-input")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void addPath();
    }
  });
  $("url-input")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void addUrl();
    }
  });
  $("chat-input")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void sendChat();
    }
  });

  window.setInterval(tickTasks, 1000);

  if (!tauriApi()) {
    setChip("请在 memTools 中打开", "err");
    $("btn-send").disabled = true;
    return;
  }

  await listenEvent("ai://event", onAiEvent);
  await listenEvent("kb://task", upsertTask);
  await listenEvent("kb://pending", () => void drainPending());
  setupDragDrop();

  try {
    const status = await call("kb_status");
    renderStatus(status);
  } catch (err) {
    setChip(messageOf(err), "err");
  }
  await loadAgents();
  try {
    const tasks = await call("kb_tasks");
    if (Array.isArray(tasks)) tasks.forEach(upsertTask);
  } catch (_) {
    /* 忽略 */
  }
  await drainPending();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => void init(), { once: true });
} else {
  void init();
}
