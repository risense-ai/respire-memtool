/* ============================================================
   memTools AI / 知识库窗口
   - 聊天：invoke ai_send → ai://event 流式回传（含思考中动态）
   - 入库：invoke kb_submit → kb://task 进度（含已用时）
   ============================================================ */

// 与其他窗口共用同一份主题（localStorage 同源），保证 AI 窗/划词窗与主窗配色一致。
import { loadTheme, applyTheme } from "./theme/engine.js";

(function bootstrapTheme() {
  try {
    const { theme } = loadTheme();
    applyTheme(theme);
  } catch (_) {
    // 主题应用失败不应阻断窗口功能，tokens.css 的浅色默认值仍在
  }
})();

const $ = (id) => document.getElementById(id);

function tauriApi() {
  const api = window.__TAURI__;
  if (!api || !api.core || typeof api.core.invoke !== "function") return null;
  return api;
}

async function call(cmd, args) {
  const api = tauriApi();
  if (!api) throw new Error("需在 memTools 中打开");
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
  presets: [],
  presetId: "",
  guideCommand: "",
  models: [],
  modelsKey: "",
  modelsError: "",
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
      pathEl.textContent = "未设置。点右上角「知识库目录」选择。";
      pathEl.className = "root-path err";
    } else {
      pathEl.textContent = root;
      pathEl.className = "root-path" + (status.root_ok ? " ok" : " err");
      pathEl.title = root;
    }
  }
  if (!status.enabled) {
    setChip("知识库已关闭，见 config.toml", "warn");
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
  if (sel && status.agent_kind) {
    const p = presetForStatus(status);
    if (p) {
      sel.value = p.id;
      state.presetId = p.id;
    }
  }
  const modelInput = $("agent-model");
  if (modelInput && document.activeElement !== modelInput) {
    modelInput.value = status.model || "";
  }
  updateAgentHint();
  renderGuide(currentPreset());
}

function updateAgentHint() {
  const hint = $("agent-hint");
  if (!hint) return;
  const preset = currentPreset();
  if (!preset) {
    hint.textContent = "选一个本机 agent，聊天与入库共用。";
    return;
  }
  const modelCount = state.models.length;
  const modelNote = modelCount ? `；模型候选 ${modelCount} 个，可下拉选或手填` : "";
  if (preset.ok) {
    const ver = preset.version ? ` ${preset.version}` : "";
    hint.textContent = `${preset.label} 可用：${preset.found || preset.bin || "自动探测"}${ver}${modelNote}`;
  } else {
    // 详细原因放下面的「安装与配置」面板，这里只留一句，别把左栏撑爆
    hint.textContent = `${preset.label} 未就绪。按下方「安装与配置」操作。`;
  }
  if (state.modelsError) {
    hint.textContent += `；模型列表获取失败：${state.modelsError}`;
  }
}

/** 选中的预设（下拉里的 id） */
function currentPreset() {
  return state.presets.find((p) => p.id === state.presetId) || null;
}

/** 绑定后回显时，按 kind+bin 找对应的预设 */
function presetForStatus(status) {
  const kind = (status?.agent_kind || "").trim().toLowerCase();
  const bin = (status?.agent_bin || "").trim().toLowerCase();
  if (!kind) return null;
  const sameKind = state.presets.filter((p) => p.kind.toLowerCase() === kind);
  if (!sameKind.length) return null;
  if (bin) {
    const exact = sameKind.find((p) => p.bin.toLowerCase() === bin);
    if (exact) return exact;
  }
  return sameKind[0];
}

/** 该预设最适合复制的一条命令（没有就退化为复制整段说明） */
function guideCommandFor(preset) {
  if (!preset) return "";
  if (preset.id === "qoder" && preset.found) return `& "${preset.found}"`;
  if (/^(npm|pnpm|yarn|brew|winget|scoop)\s/i.test(preset.install)) return preset.install;
  return "";
}

/** 未就绪时显示「怎么装、怎么登录、怎么配」 */
function renderGuide(preset) {
  const box = $("agent-guide");
  const title = $("agent-guide-title");
  const body = $("agent-guide-body");
  if (!box || !title || !body) return;
  state.guideCommand = "";
  if (!preset || preset.ok) {
    box.classList.add("hidden");
    body.textContent = "";
    title.textContent = "";
    return;
  }
  const lines = [];
  if (preset.message) lines.push(`原因：${preset.message}`);
  if (preset.install) lines.push(`安装：${preset.install}`);
  if (preset.login) lines.push(`登录：${preset.login}`);
  if (preset.id === "qoder" && preset.found) {
    lines.push(`启动：& "${preset.found}"`);
  }
  if (preset.note) lines.push(`说明：${preset.note}`);
  if (preset.kind === "custom" && preset.args?.length) {
    lines.push(`参数：${preset.args.join(" ")}（输出解析 ${preset.output || "text"}）`);
  }
  if (preset.found) lines.push(`已找到：${preset.found}`);
  if (preset.docs) lines.push(`文档：${preset.docs}`);
  lines.push("装好后点「重新检测」，再点「绑定」。");
  state.guideCommand = guideCommandFor(preset);
  title.textContent = `${preset.label} · 安装与配置`;
  body.textContent = lines.join("\n");
  box.classList.remove("hidden");
}

/** 复制安装命令（没有安装命令时退化为复制整段说明） */
async function copyGuideCommand() {
  const text = state.guideCommand || $("agent-guide-body")?.textContent || "";
  if (!text) return;
  const btn = $("btn-guide-copy");
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
    } else {
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.append(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    if (btn) {
      const old = btn.textContent;
      btn.textContent = "已复制";
      window.setTimeout(() => (btn.textContent = old), 1200);
    }
  } catch (err) {
    appendMessage("error", "复制失败：" + messageOf(err));
  }
}

/** 拉取某 agent 的模型候选，填进 datalist；结果按 kind+bin 缓存 */
async function loadModels(kind, bin) {
  const listEl = $("agent-model-options");
  if (!listEl) return [];
  const key = `${kind || ""}|${bin || ""}`;
  if (!kind) {
    state.models = [];
    state.modelsError = "";
    state.modelsKey = "";
    listEl.innerHTML = "";
    updateAgentHint();
    return [];
  }
  if (state.modelsKey === key) return state.models;
  try {
    const list = await call("agent_models", { kind, bin: bin || "" });
    state.models = Array.isArray(list) ? list : [];
    state.modelsError = "";
  } catch (err) {
    state.models = [];
    state.modelsError = messageOf(err);
  }
  state.modelsKey = key;
  listEl.innerHTML = "";
  for (const m of state.models) {
    const opt = document.createElement("option");
    opt.value = m;
    listEl.append(opt);
  }
  updateAgentHint();
  return state.models;
}

async function loadPresets() {
  const sel = $("agent-select");
  try {
    const list = await call("agent_presets");
    state.presets = Array.isArray(list) ? list : [];
    if (!sel) return;
    // 记住用户正在看的项；首次按当前绑定回填
    const keep = state.presetId;
    const bound = presetForStatus(state.status);
    sel.innerHTML = "";
    for (const p of state.presets) {
      const opt = document.createElement("option");
      opt.value = p.id;
      opt.textContent = p.ok
        ? `${p.label} · ${p.version || "已安装"}`
        : `${p.label} · 未就绪`;
      opt.title = p.ok ? p.found || p.bin : p.message || "";
      sel.append(opt);
    }
    const next = keep || bound?.id || state.presets[0]?.id || "";
    if (next) sel.value = next;
    state.presetId = sel.value;
    updateAgentHint();
    renderGuide(currentPreset());
    // 模型候选可能较慢（opencode 要起子进程），不阻塞界面
    const p = currentPreset();
    void loadModels(p?.kind || "", p?.bin || "");
  } catch (err) {
    const hint = $("agent-hint");
    if (hint) hint.textContent = "读取 agent 列表失败：" + messageOf(err);
  }
}

async function applyAgent() {
  const sel = $("agent-select");
  if (!sel || !sel.value) return;
  const preset = currentPreset();
  if (!preset) return;
  const prev = state.status?.agent_kind || "";
  // 切换来源时清空模型：opencode 是 provider/model，qoder 是档位，claude 是模型名，不能混用
  const switched = preset.kind !== prev;
  const model = switched ? "" : ($("agent-model")?.value || "").trim();
  if (!preset.ok) {
    appendMessage(
      "error",
      `${preset.label} 未就绪：${preset.message || "按左侧面板安装后再绑定"}`
    );
    renderGuide(preset);
    return;
  }
  // 先把该 agent 的模型候选换好，避免下拉里还是上一个 agent 的模型
  void loadModels(preset.kind, preset.bin);
  try {
    const status = await call("kb_set_agent", {
      kind: preset.kind,
      bin: preset.bin,
      model,
      args: preset.args || [],
      env: preset.env || [],
      output: preset.output || "",
    });
    renderStatus(status);
    const label = `已绑定 ${preset.label}${status.model ? " · " + status.model : "，默认模型"}`;
    setChip(label, "ok");
    appendMessage("tool", `已切换为 ${preset.label}${status.model ? " · " + status.model : ""}`);
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
      appendMessage("error", payload.text || "出错");
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
      enabled ? "已注册资源管理器右键菜单；Win11 在「显示更多选项」里" : "已移除右键菜单"
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
  $("btn-agent-refresh")?.addEventListener("click", () => void loadPresets());
  $("btn-guide-copy")?.addEventListener("click", () => void copyGuideCommand());
  // 换来源只预览说明，不立即绑定；确认后点「绑定」
  $("agent-select")?.addEventListener("change", () => {
    const sel = $("agent-select");
    state.presetId = sel?.value || "";
    const preset = currentPreset();
    state.modelsKey = ""; // 换来源必须重拉模型候选
    state.models = [];
    // 模型语义按来源不同（provider/model、档位、模型名），换来源就清空，别把上一个带过去
    if (preset && preset.kind !== (state.status?.agent_kind || "")) {
      const modelInput = $("agent-model");
      if (modelInput) modelInput.value = "";
    }
    updateAgentHint();
    renderGuide(preset);
    void loadModels(preset?.kind || "", preset?.bin || "");
  });
  $("agent-model")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void applyAgent();
    }
  });
  // 点进模型框时若候选为空（首次加载失败/未加载），补拉一次
  $("agent-model")?.addEventListener("focus", () => {
    const preset = currentPreset();
    if (!preset) return;
    if (state.modelsKey !== `${preset.kind}|${preset.bin}`) {
      void loadModels(preset.kind, preset.bin);
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
    setChip("需在 memTools 中打开", "err");
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
  await loadPresets();
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
