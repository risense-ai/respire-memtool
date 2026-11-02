//! 知识库入库 / 调研 / AI 对话的编排层（P13）
//!
//! 分工：
//! - 确定性动作（建目录、复制原件、落盘校验、事件推送）在 Rust 里做；
//! - 理解与写作（起标题、写 Markdown、按 rsrs 规范写记忆）交给 agent；
//! - 传输复用 [`crate::agent`] 的 `opencode run --format json`，逐行回传进度。
//!
//! 任务串行执行（同一个 `run_lock`），避免同时开多个 agent 会话把成本和磁盘打满。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::agent::{self, Request, StreamEvent};
use crate::agent_workspace;
use crate::config::{AgentConfig, KnowledgeBaseConfig, ProcessorConfig};
use crate::kb::{self, IngestRequest, Kind, Limits};
use crate::kb_settings::KbSettings;
use crate::processor_settings::ProcessorSettings;

/// 任务事件名（AI 窗口监听）
pub const EVENT_TASK: &str = "kb://task";
/// 有待处理路径（右键菜单 / 扩展送来的）
pub const EVENT_PENDING: &str = "kb://pending";
/// AI 对话事件
pub const EVENT_AI: &str = "ai://event";

/// 事件出口：把事件名与 JSON 负载交给宿主（生产环境是 Tauri emit，
/// 测试与无界面模式可注入空实现，避免把 GUI 运行时链接进测试二进制）
pub type EmitFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// 一条入库任务（前端直接渲染）
#[derive(Debug, Clone, Serialize)]
pub struct TaskRecord {
    pub id: String,
    pub kind: String,
    /// queued | running | done | failed
    pub status: String,
    /// 当前阶段（中文短句）
    pub stage: String,
    pub message: String,
    pub title: String,
    pub item_dir: String,
    pub md_path: String,
    pub memory_id: String,
    pub files: usize,
    pub bytes: u64,
    pub started_at: String,
    pub finished_at: String,
}

/// 知识库状态（AI 窗口头部）
#[derive(Debug, Clone, Serialize)]
pub struct KbStatus {
    pub enabled: bool,
    pub root: String,
    pub root_ok: bool,
    pub context_menu: bool,
    pub model: String,
    pub agent_kind: String,
    pub agent_bin: String,
    pub research_websearch: bool,
    pub max_file_mb: u64,
    pub max_files: usize,
    pub timeout_secs: u64,
    pub platform_supported: bool,
}

/// AI 会话状态
struct AiState {
    session: String,
    busy: bool,
    cancel: Option<Arc<AtomicBool>>,
}

/// 编排服务（全局单例，放进 Tauri state）
pub struct KbService {
    emit: EmitFn,
    base: KnowledgeBaseConfig,
    settings: Arc<KbSettings>,
    processor: Arc<ProcessorSettings>,
    /// config.toml 的 `[processor]`（agent 的 kind/bin/token 等默认值）
    processor_base: ProcessorConfig,
    agent_base: AgentConfig,
    workspace_base: PathBuf,
    tasks: Mutex<Vec<TaskRecord>>,
    pending: Mutex<Vec<IngestRequest>>,
    seq: AtomicU64,
    /// 入库任务串行锁
    run_lock: Mutex<()>,
    ai: Mutex<AiState>,
}

impl KbService {
    pub fn new(
        emit: EmitFn,
        base: KnowledgeBaseConfig,
        settings: Arc<KbSettings>,
        processor: Arc<ProcessorSettings>,
        processor_base: ProcessorConfig,
        agent_base: AgentConfig,
        workspace_base: PathBuf,
    ) -> Self {
        Self {
            emit,
            base,
            settings,
            processor,
            processor_base,
            agent_base,
            workspace_base,
            tasks: Mutex::new(Vec::new()),
            pending: Mutex::new(Vec::new()),
            seq: AtomicU64::new(1),
            run_lock: Mutex::new(()),
            ai: Mutex::new(AiState {
                session: String::new(),
                busy: false,
                cancel: None,
            }),
        }
    }

    /// 生效配置 = config.toml + knowledge_base.json
    pub fn effective(&self) -> KnowledgeBaseConfig {
        self.settings.effective(&self.base)
    }

    pub fn settings(&self) -> &Arc<KbSettings> {
        &self.settings
    }

    pub fn status(&self) -> KbStatus {
        let cfg = self.effective();
        let proc_cfg = self.processor.effective(&self.processor_base);
        let root = cfg.root.trim().to_string();
        let root_ok = !root.is_empty() && Path::new(&root).is_dir();
        KbStatus {
            enabled: cfg.enabled,
            root,
            root_ok,
            context_menu: cfg.context_menu,
            model: cfg.effective_model(&proc_cfg.agent.model),
            agent_kind: proc_cfg.agent.kind.clone(),
            agent_bin: proc_cfg.agent.bin.clone(),
            research_websearch: cfg.research_websearch,
            max_file_mb: cfg.max_file_mb,
            max_files: cfg.max_files,
            timeout_secs: cfg.timeout_secs,
            platform_supported: cfg!(windows),
        }
    }

    /// 设置根目录（校验可写后落盘）
    pub fn set_root(&self, root: &str) -> Result<KbStatus, String> {
        let path = kb::validate_root(root)?;
        self.settings.patch(|c| {
            c.root = path.to_string_lossy().to_string();
        })?;
        Ok(self.status())
    }

    /// 右键菜单 / 扩展送来的待处理请求
    pub fn push_pending(&self, req: IngestRequest) {
        let count = {
            let mut guard = match self.pending.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            guard.push(req);
            guard.len()
        };
        self.emit_event(EVENT_PENDING, serde_json::json!({ "count": count }));
    }

    pub fn take_pending(&self) -> Vec<IngestRequest> {
        let mut guard = match self.pending.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        std::mem::take(&mut *guard)
    }

    pub fn tasks(&self) -> Vec<TaskRecord> {
        match self.tasks.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        }
    }

    /// 提交一条入库任务；立即返回记录，实际执行在后台线程串行进行
    pub fn submit(self: &Arc<Self>, req: IngestRequest) -> Result<TaskRecord, String> {
        let cfg = self.effective();
        if !cfg.enabled {
            return Err("知识库入库未启用（config.toml [knowledge_base] enabled = false）".into());
        }
        let kind = req.resolved_kind()?;
        let id = format!(
            "kb{}-{}",
            self.seq.fetch_add(1, Ordering::Relaxed),
            chrono::Local::now().format("%H%M%S")
        );
        let record = TaskRecord {
            id: id.clone(),
            kind: kind.as_str().to_string(),
            status: "queued".into(),
            stage: "排队中".into(),
            message: String::new(),
            title: String::new(),
            item_dir: String::new(),
            md_path: String::new(),
            memory_id: String::new(),
            files: 0,
            bytes: 0,
            started_at: now_text(),
            finished_at: String::new(),
        };
        self.push_task(record.clone());
        let svc = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name(format!("memtools-kb-{id}"))
            .spawn(move || svc.run_task(id, req));
        if let Err(e) = spawned {
            self.finish_task(&record.id, "failed", &format!("无法启动入库线程：{e}"), "");
            return Err(format!("无法启动入库线程：{e}"));
        }
        Ok(record)
    }

    /// 一轮 AI 对话（流式回传）
    pub fn ai_send(self: &Arc<Self>, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("内容为空".into());
        }
        let (session, cancel) = {
            let mut guard = match self.ai.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            if guard.busy {
                return Err("上一条还在生成，请先停止或等待完成".into());
            }
            guard.busy = true;
            let flag = Arc::new(AtomicBool::new(false));
            guard.cancel = Some(Arc::clone(&flag));
            (guard.session.clone(), flag)
        };

        let svc = Arc::clone(self);
        let prompt = format!(
            "{text}\n\n（系统提示：你是 memTools 桌面端的内置助手，回答用中文、简洁、给可执行结论。\
需要长期保存的内容，提醒用户用「添加到知识库」。不要修改任何文件，除非用户明确要求。\
不要在回答里输出记忆检索过程、命中条数、「未新增记忆」这类内部记录说明。）"
        );
        let spawned = std::thread::Builder::new()
            .name("memtools-ai".into())
            .spawn(move || svc.run_chat(prompt, session, cancel));
        if let Err(e) = spawned {
            self.set_ai_busy(false, None);
            return Err(format!("无法启动 AI 线程：{e}"));
        }
        Ok(())
    }

    /// 停止当前生成
    pub fn ai_abort(&self) {
        let guard = match self.ai.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        if let Some(flag) = guard.cancel.as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// 开新对话（清空会话 id；不影响正在跑的请求）
    pub fn ai_reset(&self) {
        let mut guard = match self.ai.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        if !guard.busy {
            guard.session.clear();
        }
        self.emit_event(
            EVENT_AI,
            serde_json::json!({ "kind": "reset", "session": guard.session }),
        );
    }

    // ------------------------------------------------------------------
    // 内部
    // ------------------------------------------------------------------

    fn emit_event<T: Serialize>(&self, event: &str, payload: T) {
        let value = serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
        (self.emit)(event, value);
    }

    fn set_ai_busy(&self, busy: bool, cancel: Option<Arc<AtomicBool>>) {
        let mut guard = match self.ai.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        guard.busy = busy;
        guard.cancel = cancel;
    }

    fn push_task(&self, record: TaskRecord) {
        let mut list = match self.tasks.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        list.push(record.clone());
        // 只保留最近 30 条，避免长期运行无限增长
        if list.len() > 30 {
            let drop = list.len() - 30;
            list.drain(0..drop);
        }
        drop(list);
        self.emit_event(EVENT_TASK, &record);
    }

    fn update_task<F>(&self, id: &str, f: F)
    where
        F: FnOnce(&mut TaskRecord),
    {
        let updated = {
            let mut list = match self.tasks.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            match list.iter_mut().find(|t| t.id == id) {
                Some(t) => {
                    f(t);
                    Some(t.clone())
                }
                None => None,
            }
        };
        if let Some(rec) = updated {
            self.emit_event(EVENT_TASK, &rec);
        }
    }

    fn finish_task(&self, id: &str, status: &str, message: &str, memory_id: &str) {
        self.update_task(id, |t| {
            t.status = status.into();
            t.stage = if status == "done" { "完成" } else { "失败" }.into();
            t.message = message.into();
            if !memory_id.is_empty() {
                t.memory_id = memory_id.into();
            }
            t.finished_at = now_text();
        });
    }

    fn agent_for(&self, cfg: &KnowledgeBaseConfig, workdir: &Path) -> AgentConfig {
        let proc_cfg = self.processor.effective(&self.processor_base);
        let mut agent = self.agent_base.clone();
        // 优先跟随梳理下拉（小窗里选过 opencode/qoder）
        if !proc_cfg.agent.kind.trim().is_empty() {
            agent.kind = proc_cfg.agent.kind.clone();
        }
        if !proc_cfg.agent.bin.trim().is_empty() {
            agent.bin = proc_cfg.agent.bin.clone();
        }
        let model = cfg.effective_model(&proc_cfg.agent.model);
        if !model.is_empty() {
            agent.model = model;
        }
        agent.workdir = workdir.to_string_lossy().to_string();
        agent.timeout_secs = cfg.timeout_secs.max(30);
        agent
    }

    fn run_task(self: Arc<Self>, id: String, req: IngestRequest) {
        let _guard = match self.run_lock.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        let cfg = self.effective();
        let kind = match req.resolved_kind() {
            Ok(k) => k,
            Err(e) => {
                self.finish_task(&id, "failed", &e, "");
                return;
            }
        };
        let root = match kb::validate_root(&cfg.root) {
            Ok(p) => p,
            Err(e) => {
                self.finish_task(&id, "failed", &e, "");
                self.emit_ai_error(&e);
                return;
            }
        };
        self.update_task(&id, |t| {
            t.status = "running".into();
            t.stage = "准备目录".into();
            t.kind = kind.as_str().into();
        });

        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let title_hint = initial_title(&req, kind);
        let item_dir = match kb::create_item_dir(&root, &title_hint, &stamp) {
            Ok(p) => p,
            Err(e) => {
                self.finish_task(&id, "failed", &e, "");
                return;
            }
        };

        // ① 原件 / 原始材料落位
        let original_dir = item_dir.join("original");
        let prep = match kind {
            Kind::File | Kind::Folder => {
                self.update_task(&id, |t| t.stage = "复制原件".into());
                kb::copy_original(
                    Path::new(req.value.trim()),
                    &item_dir,
                    &root,
                    Limits::from_config(&cfg),
                )
            }
            Kind::Text | Kind::Url => {
                self.update_task(&id, |t| t.stage = "保存原始材料".into());
                write_raw_material(&item_dir, &req, kind).map(|_| {
                    let name = if kind == Kind::Text {
                        "selection.txt"
                    } else {
                        "source-url.txt"
                    };
                    (
                        original_dir.clone(),
                        vec![name.to_string()],
                        req.value.len() as u64,
                        Vec::new(),
                    )
                })
            }
        };
        let (copied, bytes, skipped) = match prep {
            Ok((_orig, files, size, skips)) => (files, size, skips),
            Err(e) => {
                self.finish_task(&id, "failed", &e, "");
                return;
            }
        };

        let md_path = item_dir.join(format!("{}.md", kb::sanitize_name(&title_hint)));
        self.update_task(&id, |t| {
            t.title = title_hint.clone();
            t.item_dir = item_dir.to_string_lossy().to_string();
            t.md_path = md_path.to_string_lossy().to_string();
            t.files = copied.len();
            t.bytes = bytes;
            t.stage = "交给 AI 整理".into();
        });

        // ② 工作区 + prompt
        let workspace = match agent_workspace::ensure(
            &self.workspace_base,
            Some(&root),
            cfg.research_websearch,
        ) {
            Ok(w) => w,
            Err(e) => {
                self.finish_task(&id, "failed", &e, "");
                return;
            }
        };
        let agent_cfg = self.agent_for(&cfg, &workspace.dir);
        let source_note = kb::source_note(&req, kind);
        // 只有调研类任务需要联网检索
        let research = cfg.research_websearch && matches!(kind, Kind::Url | Kind::Text);
        let prompt = build_prompt(
            &agent_cfg.kind,
            kind,
            &item_dir,
            &md_path,
            &title_hint,
            &source_note,
            &req,
            &copied,
            &skipped,
            research,
        );

        self.update_task(&id, |t| t.message = "AI 正在读写内容（首次可能较慢）".into());
        let outcome = agent::ask_streaming(
            &agent_cfg,
            Request::new(&prompt),
            &mut |ev| match ev {
                StreamEvent::Tool(tool) => {
                    let stage = match tool.as_str() {
                        "read" => "AI 读取资料",
                        "edit" | "write" | "patch" => "AI 写 Markdown",
                        "shell" => "AI 写记忆",
                        "webfetch" | "websearch" => "AI 联网核查",
                        _ => "AI 处理中",
                    };
                    self.update_task(&id, |t| t.stage = stage.into());
                }
                StreamEvent::Error(msg) => {
                    self.update_task(&id, |t| t.message = msg.clone());
                }
                StreamEvent::Text(_) => {}
            },
        );

        let reply = match outcome {
            Ok(r) => r,
            Err(e) => {
                self.finish_task(&id, "failed", &e, "");
                self.emit_ai_error(&e);
                return;
            }
        };

        // ③ 校验 Markdown（agent 可能写偏；只认 item_dir 内的文件）
        self.update_task(&id, |t| t.stage = "校验文档".into());
        let md = match resolve_markdown(&item_dir, &md_path) {
            Ok(p) => p,
            Err(e) => {
                let tail: String = reply.text.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
                self.finish_task(
                    &id,
                    "failed",
                    &format!("{e}；AI 末尾输出：{}", tail.trim()),
                    "",
                );
                return;
            }
        };
        let parsed = parse_result(&reply.text);

        // ④ 用 AI 给的标题重命名（不覆盖已有文件）
        let final_md = rename_with_title(&item_dir, &md, parsed.title.as_deref());

        let memory = parsed.memory_id.clone().unwrap_or_default();
        let title = parsed
            .title
            .clone()
            .unwrap_or_else(|| title_hint.clone());
        // AI 可能抄错 id，也可能漏掉 KB_RESULT：回查 rsrs 再定
        self.update_task(&id, |t| t.stage = "核对记忆".into());
        let verified = verify_memory(&memory, &item_dir, &title);
        let memory = verified.clone().unwrap_or(memory);
        let message = if memory.is_empty() {
            "Markdown 已生成，但没找到对应记忆：请检查 rsrs 登录与记忆写入".to_string()
        } else if verified.is_none() {
            "Markdown 已生成；记忆 id 未核实（AI 回报的 id 查不到），请在最近记忆中确认".to_string()
        } else {
            format!("已入库：{}", parsed.summary.clone().unwrap_or_default())
        };

        self.update_task(&id, |t| {
            t.title = title;
            t.md_path = final_md.to_string_lossy().to_string();
            t.files = copied.len();
            t.memory_id = memory.clone();
        });
        self.finish_task(&id, "done", &message, &memory);
        self.emit_event(
            "kb://done",
            serde_json::json!({
                "id": id,
                "title": parsed.title,
                "md_path": final_md.to_string_lossy(),
                "memory_id": memory,
                "session": reply.session,
            }),
        );
    }

    fn run_chat(self: Arc<Self>, prompt: String, session: String, cancel: Arc<AtomicBool>) {
        let cfg = self.effective();
        let workspace = match agent_workspace::ensure(
            &self.workspace_base,
            if cfg.root_configured() {
                Some(Path::new(cfg.root.trim()))
            } else {
                None
            },
            cfg.research_websearch,
        ) {
            Ok(w) => w,
            Err(e) => {
                self.set_ai_busy(false, None);
                self.emit_ai_error(&e);
                return;
            }
        };
        let mut agent_cfg = self.agent_for(&cfg, &workspace.dir);
        if agent_cfg.timeout_secs < 120 {
            agent_cfg.timeout_secs = 240;
        }
        let mut session_id = session;
        self.emit_event(
            EVENT_AI,
            serde_json::json!({ "kind": "start", "session": session_id }),
        );

        let req = Request::new(&prompt)
            .session(&session_id)
            .cancel(cancel.as_ref());
        let reply = agent::ask_streaming(&agent_cfg, req, &mut |ev| match ev {
            StreamEvent::Text(t) => {
                self.emit_event(
                    EVENT_AI,
                    serde_json::json!({ "kind": "text", "text": t }),
                );
            }
            StreamEvent::Tool(tool) => {
                let label = match tool.as_str() {
                    "read" => "读取文件",
                    "edit" | "write" | "patch" => "修改文件",
                    "shell" => "执行命令",
                    "webfetch" => "抓取网页",
                    "websearch" => "联网搜索",
                    "skill" => "加载技能",
                    _ => "调用工具",
                };
                self.emit_event(
                    EVENT_AI,
                    serde_json::json!({ "kind": "tool", "text": format!("{label}（{tool}）") }),
                );
            }
            StreamEvent::Error(msg) => {
                self.emit_event(
                    EVENT_AI,
                    serde_json::json!({ "kind": "error", "text": msg }),
                );
            }
        });

        match reply {
            Ok(r) => {
                if !r.session.is_empty() {
                    session_id = r.session;
                }
                let mut guard = match self.ai.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                guard.session = session_id.clone();
                guard.busy = false;
                guard.cancel = None;
                drop(guard);
                self.emit_event(
                    EVENT_AI,
                    serde_json::json!({
                        "kind": "done",
                        "session": session_id,
                        "elapsed_ms": r.elapsed_ms,
                        "bin": r.bin,
                    }),
                );
            }
            Err(e) => {
                self.set_ai_busy(false, None);
                let cancelled = e.contains("已取消");
                self.emit_event(
                    EVENT_AI,
                    serde_json::json!({ "kind": if cancelled { "done" } else { "error" }, "text": e }),
                );
            }
        }
    }

    fn emit_ai_error(&self, message: &str) {
        self.emit_event(
            EVENT_AI,
            serde_json::json!({ "kind": "error", "text": message }),
        );
    }
}

/// 未给标题时的临时标题（目录名要用）
fn initial_title(req: &IngestRequest, kind: Kind) -> String {
    let given = req.title.trim();
    if !given.is_empty() {
        return kb::sanitize_name(given);
    }
    match kind {
        Kind::File | Kind::Folder => {
            let p = Path::new(req.value.trim());
            let name = if kind == Kind::File {
                p.file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "入库".into())
            } else {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "入库".into())
            };
            kb::sanitize_name(&name)
        }
        Kind::Url => {
            let source = req.source.trim();
            if !source.is_empty() {
                kb::sanitize_name(&source.chars().take(24).collect::<String>())
            } else {
                let host = host_of(&req.value);
                kb::sanitize_name(if host.is_empty() { "网页笔记" } else { &host })
            }
        }
        Kind::Text => {
            let first = req.value.trim().lines().next().unwrap_or("").trim();
            let brief: String = first.chars().take(24).collect();
            kb::sanitize_name(if brief.is_empty() { "划词笔记" } else { &brief })
        }
    }
}

fn host_of(url: &str) -> String {
    url.split("//")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// 文本 / 网址的原始材料落盘（便于以后回看与再整理）
fn write_raw_material(item_dir: &Path, req: &IngestRequest, kind: Kind) -> Result<(), String> {
    let dir = item_dir.join("original");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("无法创建原件目录 {}：{e}", dir.display()))?;
    let (name, body) = match kind {
        Kind::Text => (
            "selection.txt",
            format!(
                "来源：{}\n\n{}",
                if req.source.trim().is_empty() {
                    "划词 / 手动输入"
                } else {
                    req.source.trim()
                },
                req.value
            ),
        ),
        _ => (
            "source-url.txt",
            format!(
                "{}\n标题：{}",
                req.value.trim(),
                if req.source.trim().is_empty() {
                    "（未提供）"
                } else {
                    req.source.trim()
                }
            ),
        ),
    };
    std::fs::write(dir.join(name), body)
        .map_err(|e| format!("写入原始材料失败：{e}"))
}

#[allow(clippy::too_many_arguments)]
fn build_prompt(
    agent_kind: &str,
    kind: Kind,
    item_dir: &Path,
    md_path: &Path,
    title_hint: &str,
    source_note: &str,
    req: &IngestRequest,
    copied: &[String],
    skipped: &[String],
    research: bool,
) -> String {
    let skill = match kind {
        Kind::File | Kind::Folder => "kb-ingest",
        _ => "kb-research",
    };
    let call = if agent_kind.trim().eq_ignore_ascii_case("opencode") {
        format!("@{skill}\n\n")
    } else {
        String::new()
    };
    let value = if kind == Kind::Url {
        req.value.trim().to_string()
    } else if kind == Kind::Text {
        req.value.chars().take(6000).collect()
    } else {
        String::new()
    };
    let files = if copied.is_empty() {
        "（无）".to_string()
    } else {
        copied
            .iter()
            .take(60)
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let skipped_text = if skipped.is_empty() {
        "（无）".to_string()
    } else {
        skipped
            .iter()
            .take(40)
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "{call}ITEM_DIR: {item}\nMD_PATH: {md}\nTITLE_HINT: {title}\nSOURCE_NOTE: {note}\n\
KIND: {kind}\nVALUE:\n{value}\n\n原文件清单：\n{files}\n\n未读取（非文本）：\n{skipped_text}\n\n\
RESEARCH_ALLOWED: {research}\n\n\
要求：\n1. 只读 ITEM_DIR 下的资料，把内容整理成一篇 Markdown，写到 MD_PATH（UTF-8），不要写别的路径。\n\
2. 标题写 12-24 字的事实句；不要用「资料」「笔记」这类空名。\n\
3. 内容要能三个月后单看就懂：结论、要点、来源、缺口。不要编造原文没有的信息。\n\
4. 完成后按 rsrs 规范写记忆：先 recall 判重，再按「改 ＞ 并 ＞ 挂 ＞ 存」处理；\
新建必须带 --parent，正文三段【前因】【行为】【后果】，【行为】里写明 MD_PATH 与 ITEM_DIR 和设备。\n\
5. 最后只输出一行：KB_RESULT: {{\"title\":\"...\",\"memory_id\":\"...\",\"summary\":\"...\"}}\n",
        item = item_dir.display(),
        md = md_path.display(),
        title = title_hint,
        note = source_note,
        kind = kind.as_str(),
    )
}

/// 校验 agent 是否真的写到了 item_dir 里；没写就找该目录下最新的 .md
fn resolve_markdown(item_dir: &Path, expected: &Path) -> Result<PathBuf, String> {
    if expected.exists() {
        let meta = std::fs::metadata(expected).map_err(|e| format!("读取 Markdown 失败：{e}"))?;
        if meta.len() > 0 {
            return Ok(expected.to_path_buf());
        }
    }
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(item_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() == 0 {
                continue;
            }
            let at = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().map(|(t, _)| at > *t).unwrap_or(true) {
                best = Some((at, path));
            }
        }
    }
    best.map(|(_, p)| p)
        .ok_or_else(|| format!("AI 没有在 {} 生成 Markdown", item_dir.display()))
}

#[derive(Debug, Default, Clone)]
struct Parsed {
    title: Option<String>,
    memory_id: Option<String>,
    summary: Option<String>,
}

/// 回查记忆 id：先 `rsrs show`，失败再按「正文含本条目录」或标题在最近条目里找。
///
/// 小模型抄 UUID 容易错一两位，也偶尔漏掉 `KB_RESULT`；直接展示假 id 比不展示更糟。
fn verify_memory(memory_id: &str, item_dir: &Path, title: &str) -> Option<String> {
    let bin = crate::rsrs_install::resolve_rsrs()?;
    if !memory_id.trim().is_empty() {
        let show = crate::rsrs_install::run_capture(
            &bin,
            &["show", memory_id, "--json"],
            std::time::Duration::from_secs(15),
        );
        if let Ok((0, out, _)) = show {
            if out.contains(memory_id) {
                return Some(memory_id.to_string());
            }
        }
    }

    let Ok((_, out, _)) = crate::rsrs_install::run_capture(
        &bin,
        &["list", "--limit", "15", "--json"],
        std::time::Duration::from_secs(20),
    ) else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(out.trim()).ok()?;
    let items = value.get("details")?.as_array()?;
    let dir = item_dir.to_string_lossy().to_lowercase();
    let want = title.trim();

    // 目录路径最可靠：技能要求【行为】里写 MD_PATH / ITEM_DIR
    for item in items {
        let content = item
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_lowercase();
        if !dir.is_empty() && content.contains(&dir) {
            if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    for item in items {
        let got = item
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim();
        if !want.is_empty() && (got == want || got.contains(want) || want.contains(got)) {
            if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

/// 解析末尾的 `KB_RESULT: {...}`
fn parse_result(text: &str) -> Parsed {
    for line in text.lines().rev() {
        let line = line.trim().trim_start_matches(['`', ' ']);
        let Some(rest) = line.strip_prefix("KB_RESULT:") else {
            continue;
        };
        let raw = rest.trim().trim_end_matches(['`', ' ']);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
            return Parsed {
                title: v
                    .get("title")
                    .and_then(|t| t.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                memory_id: v
                    .get("memory_id")
                    .and_then(|t| t.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                summary: v
                    .get("summary")
                    .and_then(|t| t.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            };
        }
    }
    Parsed::default()
}

/// 按 AI 给的标题重命名；目标已存在则保留原名
fn rename_with_title(item_dir: &Path, md: &Path, title: Option<&str>) -> PathBuf {
    let Some(title) = title else {
        return md.to_path_buf();
    };
    let clean = kb::sanitize_name(title);
    if clean.is_empty() {
        return md.to_path_buf();
    }
    let target = item_dir.join(format!("{clean}.md"));
    if target == md || target.exists() {
        return md.to_path_buf();
    }
    match std::fs::rename(md, &target) {
        Ok(()) => target,
        Err(_) => md.to_path_buf(),
    }
}

fn now_text() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(kind: &str, value: &str, title: &str) -> IngestRequest {
        IngestRequest {
            kind: kind.into(),
            value: value.into(),
            title: title.into(),
            source: String::new(),
        }
    }

    #[test]
    fn initial_title_falls_back_to_file_name() {
        let r = req("file", r"D:\docs\Rust 所有权.md", "");
        assert_eq!(initial_title(&r, Kind::File), "Rust 所有权");
        let r = req("file", r"D:\docs\无扩展名", "");
        assert_eq!(initial_title(&r, Kind::File), "无扩展名");
        let r = req("folder", r"D:\docs\项目资料", "");
        assert_eq!(initial_title(&r, Kind::Folder), "项目资料");
        let r = req("text", "第一行标题\n正文", "");
        assert_eq!(initial_title(&r, Kind::Text), "第一行标题");
        let r = req("url", "https://example.com/a/b", "");
        assert_eq!(initial_title(&r, Kind::Url), "example.com");
        let r = req("file", r"D:\a.md", "自定义");
        assert_eq!(initial_title(&r, Kind::File), "自定义");
    }

    #[test]
    fn result_parsing_reads_last_line_and_ignores_garbage() {
        let text = "前言\nKB_RESULT: {\"title\":\"Rust 所有权\",\"memory_id\":\"abc123\",\"summary\":\"要点\"}\n";
        let got = parse_result(text);
        assert_eq!(got.title.as_deref(), Some("Rust 所有权"));
        assert_eq!(got.memory_id.as_deref(), Some("abc123"));
        assert_eq!(got.summary.as_deref(), Some("要点"));

        // 代码块包裹也能认
        let got = parse_result("```\nKB_RESULT: {\"title\":\"T\"}\n```");
        assert_eq!(got.title.as_deref(), Some("T"));

        assert!(parse_result("没有结果").title.is_none());
        assert!(parse_result("KB_RESULT: not-json").title.is_none());
    }

    #[test]
    fn prompt_contains_paths_and_skill() {
        let item = Path::new(r"D:\kb\2026\11\a-20261102-101010");
        let md = item.join("笔记.md");
        let r = req("file", r"D:\src\a.md", "笔记");
        let p = build_prompt(
            "opencode",
            Kind::File,
            item,
            &md,
            "笔记",
            "",
            &r,
            &["a.md".to_string()],
            &[],
            true,
        );
        assert!(p.starts_with("@kb-ingest"));
        assert!(p.contains(r"ITEM_DIR: D:\kb\2026\11\a-20261102-101010"));
        assert!(p.contains("MD_PATH:"));
        assert!(p.contains("RESEARCH_ALLOWED: true"));
        assert!(p.contains("KB_RESULT:"));

        // 非 opencode 不加 @skill
        let q = build_prompt("qoder", Kind::Url, item, &md, "t", "", &r, &[], &[], false);
        assert!(!q.starts_with("@kb"));
        assert!(q.contains("RESEARCH_ALLOWED: false"));
    }

    #[test]
    fn resolve_markdown_prefers_expected_then_falls_back() {
        let dir = std::env::temp_dir().join(format!(
            "memtools-kb-resolve-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let expected = dir.join("a.md");
        assert!(resolve_markdown(&dir, &expected).is_err());
        std::fs::write(&expected, "# a").unwrap();
        assert_eq!(resolve_markdown(&dir, &expected).unwrap(), expected);
        std::fs::write(&expected, "").unwrap();
        std::fs::write(dir.join("other.md"), "x").unwrap();
        assert_eq!(
            resolve_markdown(&dir, &expected).unwrap(),
            dir.join("other.md")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_uses_title_and_keeps_existing() {
        let dir = std::env::temp_dir().join(format!(
            "memtools-kb-rename-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("old.md");
        std::fs::write(&md, "x").unwrap();
        let got = rename_with_title(&dir, &md, Some("新标题"));
        assert_eq!(got.file_name().unwrap().to_string_lossy(), "新标题.md");
        assert!(got.exists());

        let md2 = dir.join("b.md");
        std::fs::write(&md2, "x").unwrap();
        std::fs::write(dir.join("冲突.md"), "y").unwrap();
        let kept = rename_with_title(&dir, &md2, Some("冲突"));
        assert_eq!(kept, md2, "目标已存在时保留原名，不覆盖");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_extraction_is_safe() {
        assert_eq!(host_of("https://example.com/a?b=1"), "example.com");
        assert_eq!(host_of("no-scheme"), "no-scheme");
    }
}
