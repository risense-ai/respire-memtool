//! 开机自启（P12）
//!
//! 用户在安装包或设置面板里开启「开机自启」后，Windows 登录时由系统拉起
//! memTools，并带上 [`AUTOSTART_ARG`]。本模块只负责：
//!
//! 1. 识别本次是否由开机自启拉起；
//! 2. 此时检查 rsrs runtime 是否在线，离线则调用 `rsrs status` 让 rsrs
//!    自行拉起常驻 runtime；
//! 3. 把检查结果推给前端（`runtime://autostart`）。
//!
//! 边界：
//! - 只在开机自启路径检查，普通手动启动不额外碰 rsrs 进程；
//! - 只调用 `rsrs status`，不停止、不重启 runtime，也不使用 `--direct`；
//! - 子进程 + 参数数组调用，不拼接 shell；不读取 rsrs 数据目录。

use std::time::Duration;

use serde::Serialize;

use crate::config::RsrsCliConfig;
use crate::rsrs_client::RsrsClient;

/// 开机自启写入的命令行参数：Rust 端据此判断是否需要检查 rsrs。
pub const AUTOSTART_ARG: &str = "--autostart";

/// 前端监听的事件名
pub const EVENT: &str = "runtime://autostart";

/// `rsrs status` 最长等待：首次拉起 runtime 可能要加载模型。
const ENSURE_TIMEOUT: Duration = Duration::from_secs(180);
/// 启动后重新探测次数（每次间隔 1s）
const PROBE_ROUNDS: usize = 6;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEnsure {
    /// 检查结束时 runtime 是否在线
    pub online: bool,
    /// 本次是否执行了「启动 rsrs」
    pub started: bool,
    /// 给用户看的一句话
    pub message: String,
    /// rsrs 命令输出尾部（失败排障用）
    pub log: Option<String>,
}

/// 本次进程是否由开机自启拉起。
pub fn launched_by_autostart() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_ARG)
}

/// 确保 rsrs runtime 在线；离线时调用 `rsrs status` 交给 rsrs 自己拉起。
///
/// 同步阻塞，调用方应放在后台线程 / `spawn_blocking` 里。
pub fn ensure_runtime(client: &RsrsClient, cfg: &RsrsCliConfig) -> RuntimeEnsure {
    match probe(client) {
        Probe::Online => {
            return RuntimeEnsure {
                online: true,
                started: false,
                message: "rsrs runtime 已在线，无需启动".into(),
                log: None,
            }
        }
        // 端口能连、但鉴权/MCP/解析失败时，问题不在「没启动」，拉起 rsrs 也修不好。
        Probe::ReachableError(err) => {
            return RuntimeEnsure {
                online: false,
                started: false,
                message: format!("rsrs runtime 可连接，但状态读取失败：{err}\n请查看 rsrs 面板。"),
                log: None,
            }
        }
        Probe::Offline => {}
    }

    let Some(bin) = crate::rsrs_install::resolve_rsrs() else {
        return RuntimeEnsure {
            online: false,
            started: false,
            message: format!(
                "未找到 rsrs 命令，无法自动启动 runtime。\n请在设置或「rsrs」面板安装 {}。",
                cfg.package
            ),
            log: None,
        };
    };

    // 不传 --client-only：让 rsrs 管理并拉起常驻 runtime。
    // 同时清掉沙盒可能注入的两个「禁止自启」开关。
    let result = crate::rsrs_install::run_capture_env(
        &bin,
        &["status", "--json"],
        ENSURE_TIMEOUT,
        &["RSRS_CLIENT_ONLY", "ONEMEMORY_NO_AUTOSTART"],
    );

    let log = match result {
        Ok((code, out, err)) => {
            if code != 0 {
                let detail = tail(if err.trim().is_empty() { &out } else { &err }, 600);
                return RuntimeEnsure {
                    online: false,
                    started: false,
                    message: format!("rsrs status 退出码 {code}，未能启动 runtime：{detail}"),
                    log: Some(detail),
                };
            }
            let out = out.trim();
            (!out.is_empty()).then(|| tail(out, 400))
        }
        Err(err) => {
            return RuntimeEnsure {
                online: false,
                started: false,
                message: format!("无法执行 rsrs status：{err}"),
                log: Some(err),
            }
        }
    };

    // `rsrs status` 返回后 runtime 通常已就绪，再留几轮给模型/端口初始化。
    for _ in 0..PROBE_ROUNDS {
        if is_online(client) {
            return RuntimeEnsure {
                online: true,
                started: true,
                message: "已启动 rsrs runtime".into(),
                log,
            };
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    RuntimeEnsure {
        online: false,
        started: true,
        message: "已执行 rsrs status，但 runtime 仍未就绪。\n请查看 rsrs 面板或稍后重试。".into(),
        log,
    }
}

/// 首次探测的三种结论：在线；确实连不上；能连上但状态读取失败。
enum Probe {
    Online,
    Offline,
    ReachableError(String),
}

fn probe(client: &RsrsClient) -> Probe {
    match client.status_info() {
        Ok(_) => Probe::Online,
        // 只有「连不上」才值得让 rsrs 自己拉起；鉴权/MCP 错误在启动路径上修不好。
        Err(crate::rsrs_client::RsrsError::Unreachable(_)) => Probe::Offline,
        Err(err) => Probe::ReachableError(err.to_string()),
    }
}

fn is_online(client: &RsrsClient) -> bool {
    matches!(probe(client), Probe::Online)
}

fn tail(text: &str, limit: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    text.chars().skip(count - limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_last_chars() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 10), "abc");
    }
}
