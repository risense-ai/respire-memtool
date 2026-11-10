//! DeskPet 状态推送：通过本机 DeskPet 的 MCP 端点把 memTools 状态显示到桌宠气泡。
//!
//! DeskPet（大肥鱼）在 `127.0.0.1:17340/mcp` 暴露 streamable-http MCP，
//! 工具 `pet_say` 会让桌宠显示一句话气泡。连接信息由 DeskPet 写在
//! `%LOCALAPPDATA%\DeskPet\control.json`（host / port / token），token 每次
//! 启动轮换，所以每次推送前都重新读取，不缓存令牌。
//!
//! 设置面板只控制开关（`deskpet.json` 叠加 `config.toml`）。推送线程默认
//! 关闭时完全不访问 DeskPet；开启后按 `interval_secs` 检查状态，只在状态
//! 变化时推送，另每 10 分钟心跳刷新一次，避免刷屏。
//!
//! 红线：只连回环地址；桌面宠物未运行时静默跳过，不影响记忆读写。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::DeskpetConfig;
use crate::deskpet_settings::DeskpetSettings;
use crate::rsrs_client::{RsrsClient, RsrsError, StatusInfo};

const PROTOCOL_VERSION: &str = "2025-06-18";
const SESSION_HEADER: &str = "Mcp-Session-Id";
const TOKEN_HEADER: &str = "X-DeskPet-Token";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const STARTUP_DELAY: Duration = Duration::from_secs(3);
const HEARTBEAT: Duration = Duration::from_secs(600);
const MAX_STATUS_CHARS: usize = 500;
const STATUS_TOOL: &str = "pet_say";

// ---------------------------------------------------------------------------
// 控制面发现
// ---------------------------------------------------------------------------

/// `control.json` 的默认位置：显式配置 > `DESKPET_CONTROL` > 系统本地应用数据目录。
pub fn resolve_control_file(config: &DeskpetConfig) -> PathBuf {
    let explicit = config.control_file.trim();
    if !explicit.is_empty() {
        return PathBuf::from(explicit);
    }
    if let Some(path) = std::env::var_os("DESKPET_CONTROL").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(base) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return PathBuf::from(base).join("DeskPet").join("control.json");
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("DeskPet")
        .join("control.json")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoint {
    host: String,
    port: u16,
    token: String,
}

#[derive(Debug, Deserialize)]
struct ControlFile {
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    token: String,
}

fn read_endpoint(path: &Path) -> Result<Endpoint, DeskpetError> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        DeskpetError::NotRunning(format!("未找到 DeskPet（{}）：{error}", path.display()))
    })?;
    let info: ControlFile = serde_json::from_str(&text)
        .map_err(|error| DeskpetError::Protocol(format!("control.json 解析失败：{error}")))?;
    let token = info.token.trim().to_string();
    if token.is_empty() {
        return Err(DeskpetError::Protocol(
            "control.json 缺少 token；请重启 DeskPet".into(),
        ));
    }
    let host = info
        .host
        .unwrap_or_else(|| "127.0.0.1".into())
        .trim()
        .to_string();
    if !is_loopback_host(&host) {
        return Err(DeskpetError::Protocol(format!(
            "control.json 的 host「{host}」不是回环地址；为避免把状态发到远端，已拒绝连接"
        )));
    }
    Ok(Endpoint {
        host,
        port: info.port.unwrap_or(17340),
        token,
    })
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// MCP 客户端
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum DeskpetError {
    /// DeskPet 未运行 / control.json 不存在
    NotRunning(String),
    /// 协议或响应不符合 MCP
    Protocol(String),
    /// DeskPet 明确拒绝（401/403、isError、JSON-RPC error）
    Rejected(String),
    /// 本会话失效，需要重新 initialize
    SessionExpired,
}

impl std::fmt::Display for DeskpetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeskpetError::NotRunning(message)
            | DeskpetError::Protocol(message)
            | DeskpetError::Rejected(message) => write!(f, "{message}"),
            DeskpetError::SessionExpired => write!(f, "DeskPet MCP 会话已失效"),
        }
    }
}

struct HttpOutcome {
    status: u16,
    session: Option<String>,
    body: String,
}

struct DeskpetClient {
    control_file: PathBuf,
    endpoint: Option<Endpoint>,
    session: Option<String>,
    agent: ureq::Agent,
}

impl DeskpetClient {
    fn new(control_file: PathBuf) -> Self {
        let agent = ureq::AgentBuilder::new()
            // 回环地址必须直连：系统代理会让 127.0.0.1 也走代理。
            .try_proxy_from_env(false)
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build();
        Self {
            control_file,
            endpoint: None,
            session: None,
            agent,
        }
    }

    /// 让桌宠显示一句话气泡。
    fn say(&mut self, text: &str) -> Result<(), DeskpetError> {
        let text = truncate_chars(text.trim(), MAX_STATUS_CHARS);
        if text.is_empty() {
            return Ok(());
        }
        match self.call_tool(json!({ "text": &text })) {
            Ok(_) => Ok(()),
            Err(DeskpetError::SessionExpired) => {
                // DeskPet 重启后 token 轮换、会话清空：重读 control.json 再试一次。
                self.session = None;
                self.endpoint = None;
                self.call_tool(json!({ "text": &text })).map(|_| ())
            }
            Err(error) => Err(error),
        }
    }

    fn call_tool(&mut self, arguments: Value) -> Result<Value, DeskpetError> {
        self.ensure_session()?;
        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": STATUS_TOOL, "arguments": arguments }
        });
        let outcome = self.post(&request, true)?;
        if outcome.status == 401 || outcome.status == 403 {
            return Err(DeskpetError::SessionExpired);
        }
        if outcome.status == 400 && outcome.body.contains("bad-session") {
            return Err(DeskpetError::SessionExpired);
        }
        if outcome.status != 200 {
            return Err(DeskpetError::Protocol(format!(
                "DeskPet MCP HTTP {}：{}",
                outcome.status,
                truncate_chars(outcome.body.trim(), 160)
            )));
        }
        let value = parse_rpc_body(&outcome.body)?;
        if let Some(error) = value.get("error") {
            return Err(DeskpetError::Rejected(format!(
                "DeskPet MCP 错误：{}",
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("未知错误")
            )));
        }
        let result = value
            .get("result")
            .ok_or_else(|| DeskpetError::Protocol("DeskPet MCP 响应缺少 result".into()))?;
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(DeskpetError::Rejected(content_text(result)));
        }
        Ok(result.clone())
    }

    fn ensure_session(&mut self) -> Result<(), DeskpetError> {
        if self.session.is_some() {
            return Ok(());
        }
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "memTools", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        let outcome = self.post(&request, false)?;
        if outcome.status == 401 || outcome.status == 403 {
            return Err(DeskpetError::Rejected(
                "DeskPet 拒绝了连接令牌；请重启 DeskPet 后重试".into(),
            ));
        }
        if outcome.status != 200 {
            return Err(DeskpetError::Protocol(format!(
                "DeskPet MCP initialize HTTP {}：{}",
                outcome.status,
                truncate_chars(outcome.body.trim(), 160)
            )));
        }
        let value = parse_rpc_body(&outcome.body)?;
        if let Some(error) = value.get("error") {
            return Err(DeskpetError::Rejected(format!(
                "DeskPet MCP 初始化失败：{}",
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("未知错误")
            )));
        }
        let session = outcome.session.ok_or_else(|| {
            DeskpetError::Protocol("DeskPet MCP initialize 未返回 Mcp-Session-Id".into())
        })?;
        self.session = Some(session);
        // 通知没有响应（DeskPet 返回 202）；失败时清掉会话，下一轮重新握手。
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let outcome = match self.post(&notification, true) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.session = None;
                return Err(error);
            }
        };
        if outcome.status == 401 || outcome.status == 403 || outcome.status == 400 {
            self.session = None;
            return Err(DeskpetError::SessionExpired);
        }
        Ok(())
    }

    fn post(&mut self, payload: &Value, with_session: bool) -> Result<HttpOutcome, DeskpetError> {
        let endpoint = self.refresh_endpoint()?;
        let url = mcp_url(&endpoint);
        let session = if with_session {
            self.session.as_deref()
        } else {
            None
        };
        let mut request = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json, text/event-stream")
            .set(TOKEN_HEADER, &endpoint.token);
        if let Some(session) = session {
            request = request.set(SESSION_HEADER, session);
        }
        match request.send_json(payload) {
            Ok(response) => Ok(HttpOutcome {
                status: response.status(),
                session: response.header(SESSION_HEADER).map(str::to_string),
                body: response.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Status(status, response)) => Ok(HttpOutcome {
                status,
                session: response.header(SESSION_HEADER).map(str::to_string),
                body: response.into_string().unwrap_or_default(),
            }),
            Err(ureq::Error::Transport(error)) => Err(DeskpetError::NotRunning(format!(
                "DeskPet 控制面不可达：{error}"
            ))),
        }
    }

    /// 每次请求前重读 control.json；DeskPet 重启后端口/token 变化会自动跟上。
    fn refresh_endpoint(&mut self) -> Result<Endpoint, DeskpetError> {
        let endpoint = read_endpoint(&self.control_file)?;
        if self.endpoint.as_ref() != Some(&endpoint) {
            self.session = None;
        }
        self.endpoint = Some(endpoint.clone());
        Ok(endpoint)
    }
}

fn mcp_url(endpoint: &Endpoint) -> String {
    let host = if endpoint.host.contains(':') && !endpoint.host.starts_with('[') {
        format!("[{}]", endpoint.host)
    } else {
        endpoint.host.clone()
    };
    format!("http://{}:{}/mcp", host, endpoint.port)
}

fn parse_rpc_body(body: &str) -> Result<Value, DeskpetError> {
    let trimmed = body.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(Value::Null);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed)
            .map_err(|error| DeskpetError::Protocol(format!("DeskPet MCP 响应解析失败：{error}")));
    }
    // 兼容 text/event-stream：取第一条 data 事件里的 JSON。
    for line in trimmed.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim_start();
        if data.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            return Ok(value);
        }
    }
    Err(DeskpetError::Protocol(
        "DeskPet MCP 响应既不是 JSON 也不是 SSE data".into(),
    ))
}

fn content_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(Value::as_str))
        })
        .map(str::to_string)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| "DeskPet 返回错误".into())
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let kept: String = value.chars().take(limit - 1).collect();
    format!("{kept}…")
}

// ---------------------------------------------------------------------------
// 状态文案
// ---------------------------------------------------------------------------

/// 把 rsrs 状态折叠成一句桌宠气泡文案（≤500 字）。
pub fn status_message(info: Result<StatusInfo, RsrsError>) -> String {
    let detail = match info {
        Ok(status) => {
            let mut parts = Vec::new();
            if status.offline.unwrap_or(false) {
                parts.push("rsrs 离线".to_string());
            } else {
                parts.push("rsrs 在线".to_string());
            }
            if status.pending > 0 {
                parts.push(format!("{} 条待同步", status.pending));
            }
            if status.conflicts > 0 {
                parts.push(format!("{} 条冲突", status.conflicts));
            }
            if !status.index_ready {
                parts.push("语义索引未就绪".to_string());
            }
            parts.join(" · ")
        }
        Err(error) => match error {
            RsrsError::Unreachable(_) => "rsrs runtime 不可达".to_string(),
            RsrsError::AuthFailed(_) => "rsrs 鉴权失败".to_string(),
            _ => "rsrs 状态读取失败".to_string(),
        },
    };
    format!("memTools：{detail}")
}

// ---------------------------------------------------------------------------
// 推送服务
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DeskpetStatusOut {
    pub enabled: bool,
    pub connected: bool,
    pub message: String,
    pub last_error: Option<String>,
}

pub struct DeskpetService {
    base: DeskpetConfig,
    settings: Arc<DeskpetSettings>,
    rsrs: Arc<RsrsClient>,
    client: Mutex<DeskpetClient>,
    /// 串行化手动推送与后台检查，避免重复弹气泡
    tick_lock: Mutex<()>,
    wake: Mutex<Option<Sender<()>>>,
    last_message: Mutex<Option<String>>,
    last_push: Mutex<Option<Instant>>,
    last_error: Mutex<Option<String>>,
    connected: Mutex<bool>,
}

impl DeskpetService {
    pub fn new(base: DeskpetConfig, settings: Arc<DeskpetSettings>, rsrs: Arc<RsrsClient>) -> Self {
        let control_file = resolve_control_file(&base);
        Self {
            base,
            settings,
            rsrs,
            client: Mutex::new(DeskpetClient::new(control_file)),
            tick_lock: Mutex::new(()),
            wake: Mutex::new(None),
            last_message: Mutex::new(None),
            last_push: Mutex::new(None),
            last_error: Mutex::new(None),
            connected: Mutex::new(false),
        }
    }

    /// 启动后台检查线程；开关关闭时线程只等待，不访问 DeskPet。
    pub fn start(self: &Arc<Self>) {
        let (sender, receiver) = channel();
        match self.wake.lock() {
            Ok(mut wake) => *wake = Some(sender),
            Err(poisoned) => *poisoned.into_inner() = Some(sender),
        }
        let service = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("memtools-deskpet".into())
            .spawn(move || service.run(receiver));
        if let Err(error) = spawned {
            eprintln!("[memTools] DeskPet 推送线程启动失败：{error}");
        }
    }

    pub fn status(&self) -> DeskpetStatusOut {
        let enabled = self.settings.is_enabled(&self.base);
        let connected = *self
            .connected
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let last_error = self
            .last_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let message = if !enabled {
            "已关闭（不会向 DeskPet 推送状态）".to_string()
        } else if connected {
            "已开启 · DeskPet 已连接".to_string()
        } else if let Some(error) = &last_error {
            format!("已开启 · 未连接 DeskPet：{}", truncate_chars(error, 120))
        } else {
            "已开启 · 等待首次推送".to_string()
        };
        DeskpetStatusOut {
            enabled,
            connected,
            message,
            last_error,
        }
    }

    /// 保存开关；开启后立即做一次推送，便于设置面板马上看到结果。
    pub fn set_enabled(&self, enabled: bool) -> Result<DeskpetStatusOut, String> {
        self.settings.set_enabled(enabled)?;
        if enabled {
            self.tick(true);
        } else {
            self.reset_runtime_state();
        }
        Ok(self.status())
    }

    fn run(&self, receiver: Receiver<()>) {
        // 启动期先让 rsrs 探测/分类树预热跑完，避免抢连接。
        let _ = receiver.recv_timeout(STARTUP_DELAY);
        loop {
            if self.settings.is_enabled(&self.base) {
                self.tick(false);
            } else {
                self.reset_runtime_state();
            }
            let wait = if self.settings.is_enabled(&self.base) {
                self.base.effective_interval()
            } else {
                // 关闭时低频醒来等待开关变化；开启会立即触发一次 tick。
                Duration::from_secs(15)
            };
            match receiver.recv_timeout(wait) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn tick(&self, force: bool) -> DeskpetStatusOut {
        let _guard = self
            .tick_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.settings.is_enabled(&self.base) {
            self.reset_runtime_state();
            return self.status();
        }
        let message = status_message(self.rsrs.status_info());
        let should_push = {
            let last_message = self
                .last_message
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let last_push = self
                .last_push
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let last_error = self
                .last_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            force
                || last_message.as_deref() != Some(message.as_str())
                || last_error.is_some()
                || last_push.map_or(true, |at| at.elapsed() >= HEARTBEAT)
        };
        if should_push {
            let result = self
                .client
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .say(&message);
            match result {
                Ok(()) => {
                    *self
                        .last_message
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(message.clone());
                    *self
                        .last_push
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
                    *self
                        .connected
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
                    let had_error = self
                        .last_error
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .take()
                        .is_some();
                    if had_error {
                        eprintln!("[memTools] DeskPet 状态推送已恢复");
                    }
                }
                Err(error) => {
                    *self
                        .connected
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
                    let message = error.to_string();
                    let previous = self
                        .last_error
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .replace(message.clone());
                    if previous.as_deref() != Some(message.as_str()) {
                        eprintln!("[memTools] DeskPet 状态推送失败：{message}");
                    }
                }
            }
        }
        self.status()
    }

    fn reset_runtime_state(&self) {
        *self
            .last_message
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .last_push
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .last_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .connected
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn online(pending: u64, conflicts: u64, index_ready: bool) -> StatusInfo {
        StatusInfo {
            online: true,
            offline: Some(false),
            pending,
            conflicts,
            index_ready,
            ..Default::default()
        }
    }

    #[test]
    fn status_message_reports_online_and_offline() {
        assert_eq!(
            status_message(Ok(online(0, 0, true))),
            "memTools：rsrs 在线"
        );
        let mut offline = online(3, 1, false);
        offline.offline = Some(true);
        let message = status_message(Ok(offline));
        assert!(message.contains("rsrs 离线"));
        assert!(message.contains("3 条待同步"));
        assert!(message.contains("1 条冲突"));
        assert!(message.contains("语义索引未就绪"));
        assert!(message.chars().count() <= MAX_STATUS_CHARS);
    }

    #[test]
    fn status_message_explains_unreachable_runtime() {
        let message = status_message(Err(RsrsError::Unreachable("down".into())));
        assert_eq!(message, "memTools：rsrs runtime 不可达");
    }

    #[test]
    fn parse_accepts_json_and_sse() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
        assert!(parse_rpc_body(json).is_ok());
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        assert!(parse_rpc_body(sse).is_ok());
    }

    #[test]
    fn control_file_rejects_non_loopback_host() {
        let dir =
            std::env::temp_dir().join(format!("memtools-deskpet-control-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("control.json");
        std::fs::write(&path, r#"{"host":"10.0.0.2","port":17340,"token":"abc"}"#).unwrap();
        assert!(read_endpoint(&path).is_err());
        std::fs::write(&path, r#"{"host":"127.0.0.1","port":17340,"token":"abc"}"#).unwrap();
        assert_eq!(read_endpoint(&path).unwrap().port, 17340);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncate_respects_character_limit() {
        let value = "状态".repeat(400);
        let out = truncate_chars(&value, MAX_STATUS_CHARS);
        assert_eq!(out.chars().count(), MAX_STATUS_CHARS);
        assert!(out.ends_with('…'));
    }

    #[derive(Default)]
    struct Seen {
        initialize_token: Option<String>,
        notification_sessions: Vec<String>,
        success_session: Option<String>,
        success_body: Option<String>,
        initializes: u32,
        tool_calls: u32,
    }

    fn request_header(request: &tiny_http::Request, name: &str) -> Option<String> {
        request
            .headers()
            .iter()
            .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str().to_string())
    }

    fn respond(request: tiny_http::Request, status: u16, body: &str, extra: Option<(&str, &str)>) {
        let mut response = tiny_http::Response::from_string(body).with_status_code(status);
        if let Some((name, value)) = extra {
            response.add_header(
                tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap(),
            );
        }
        let _ = request.respond(response);
    }

    /// 模拟 DeskPet 的 streamable-http MCP：
    /// 第一次 tools/call 返回 bad-session，客户端应重新 initialize 后重试成功。
    #[test]
    fn mcp_client_initializes_and_retries_after_deskpet_restart() {
        use std::sync::{Arc, Mutex};

        let server = Arc::new(
            tiny_http::Server::http(("127.0.0.1", 0)).expect("mock MCP server should bind"),
        );
        let port = server
            .server_addr()
            .to_ip()
            .expect("ephemeral port should be IP")
            .port();
        let seen = Arc::new(Mutex::new(Seen::default()));

        let worker = {
            let server = Arc::clone(&server);
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                for mut request in server.incoming_requests() {
                    let token = request_header(&request, TOKEN_HEADER);
                    let session = request_header(&request, SESSION_HEADER);
                    let mut body = String::new();
                    let _ = request.as_reader().read_to_string(&mut body);
                    let message: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    match message.get("method").and_then(Value::as_str) {
                        Some("initialize") => {
                            let mut seen = seen.lock().unwrap();
                            seen.initializes += 1;
                            seen.initialize_token = token;
                            let session_id = format!("sess-{}", seen.initializes);
                            drop(seen);
                            respond(
                                request,
                                200,
                                r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"deskpet","version":"test"}}}"#,
                                Some((SESSION_HEADER, &session_id)),
                            );
                        }
                        Some("notifications/initialized") => {
                            if let Some(session) = session {
                                seen.lock().unwrap().notification_sessions.push(session);
                            }
                            let _ = request.respond(tiny_http::Response::empty(202));
                        }
                        Some("tools/call") => {
                            let mut seen = seen.lock().unwrap();
                            seen.tool_calls += 1;
                            let first = seen.tool_calls == 1;
                            if first {
                                drop(seen);
                                respond(
                                    request,
                                    400,
                                    r#"{"ok":false,"reason":"bad-session"}"#,
                                    None,
                                );
                            } else {
                                seen.success_session = session;
                                seen.success_body = Some(body.clone());
                                drop(seen);
                                respond(
                                    request,
                                    200,
                                    r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"已显示"}],"structuredContent":{"ok":true}}}"#,
                                    None,
                                );
                            }
                        }
                        _ => respond(request, 404, "{}", None),
                    }
                }
            })
        };

        let dir =
            std::env::temp_dir().join(format!("memtools-deskpet-mock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let control = dir.join("control.json");
        std::fs::write(
            &control,
            format!(r#"{{"host":"127.0.0.1","port":{port},"token":"secret-token"}}"#),
        )
        .unwrap();

        let mut client = DeskpetClient::new(control);
        client
            .say("memTools：rsrs 在线")
            .expect("bad-session 后应重新 initialize 并重试成功");

        server.unblock();
        worker.join().unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.initializes, 2);
        assert_eq!(seen.tool_calls, 2);
        assert_eq!(seen.initialize_token.as_deref(), Some("secret-token"));
        assert_eq!(seen.notification_sessions, vec!["sess-1", "sess-2"]);
        assert_eq!(seen.success_session.as_deref(), Some("sess-2"));
        let body = seen.success_body.as_deref().unwrap();
        assert!(body.contains("\"pet_say\""));
        assert!(body.contains("memTools：rsrs 在线"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
