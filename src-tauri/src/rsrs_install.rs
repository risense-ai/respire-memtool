//! rsrs CLI 检测 / 安装 / 更新（P11）
//!
//! 为什么需要：memTools 只通过 MCP over HTTP 连接 rsrs runtime；全新机器上
//! 若连 `rsrs` 命令都没有，用户会一直卡在「runtime 不可达」。本模块负责：
//!
//! 1. 检测本机 `rsrs` 命令（PATH + npm/pnpm 常见全局目录）；
//! 2. 未安装时用包管理器全局安装官方 CLI（默认 `rsrs-cli`，失败回退 `@rsrsai/cli`）；
//! 3. 查询 registry 最新版本，供界面「手动更新」使用；
//! 4. 把安装/更新进度以 `rsrs://install` 事件推给前端。
//!
//! 边界（与项目红线一致）：
//! - 只安装/升级 CLI 包，**从不启动、停止或重启 rsrs runtime**，也不使用 `--direct`；
//! - 子进程 + 参数数组调用 npm/pnpm，不拼接 shell 字符串；
//! - 不读取、不修改 rsrs 数据目录，不输出令牌等敏感信息。

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::RsrsCliConfig;

/// 安装/更新进度事件名（前端监听）
const EVENT: &str = "rsrs://install";
/// 单次安装最长等待：npm 全局安装在慢网络下可能数分钟
const INSTALL_TIMEOUT: Duration = Duration::from_secs(900);

/// 同一时刻只允许一个安装/更新任务
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
pub struct CliStatus {
    /// 是否已安装且能执行 `rsrs --version`
    pub installed: bool,
    /// 解析出的版本号（如 1.0.16）
    pub version: Option<String>,
    /// 实际使用的可执行文件
    pub bin: Option<String>,
    /// 可用的包管理器（npm / pnpm）
    pub package_manager: Option<String>,
    /// 能否自动安装（有包管理器 = 能）
    pub can_install: bool,
    /// 配置里的首次自动安装开关
    pub auto_install: bool,
    /// 配置里的主包名
    pub package: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    pub installed: Option<String>,
    pub latest: String,
    pub has_update: bool,
    pub package: String,
    pub package_manager: String,
}

#[derive(Debug, Clone, Serialize)]
struct Progress {
    /// 阶段名（准备 / 安装中 / 校验 / 完成 / 失败）
    stage: String,
    /// 0-100
    progress: u8,
    /// 给用户看的一句话
    message: String,
    /// npm/pnpm 最新一行输出
    log: Option<String>,
    done: bool,
    ok: bool,
    version: Option<String>,
}

struct PackageManager {
    name: String,
    bin: PathBuf,
}

/// 释放 RUNNING 标记（任何提前返回都不会卡死后续安装）
struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// 检测
// ---------------------------------------------------------------------------

/// 检测 `rsrs` 命令与包管理器。同步阻塞，调用方放在 blocking 线程里执行。
pub fn status(cfg: &RsrsCliConfig) -> CliStatus {
    let bin = resolve_rsrs();
    let mut version = None;
    let mut installed = false;

    if let Some(path) = bin.as_ref() {
        if let Ok((code, out, err)) = run_capture(path, &["--version"], Duration::from_secs(20)) {
            if code == 0 {
                version = parse_version(&out).or_else(|| parse_version(&err));
                installed = version.is_some() || !out.trim().is_empty();
            }
        }
    }

    let pm = find_pm();
    let package = pm
        .as_ref()
        .and_then(|p| installed_package(p, cfg))
        .unwrap_or_else(|| cfg.package.trim().to_string());
    CliStatus {
        installed,
        version,
        bin: bin.map(|p| p.display().to_string()),
        package_manager: pm.as_ref().map(|p| p.name.clone()),
        can_install: pm.is_some(),
        auto_install: cfg.auto_install,
        package,
    }
}

/// PATH 优先，其次 npm / pnpm 的常见全局 bin 目录。
pub(crate) fn resolve_rsrs() -> Option<PathBuf> {
    if let Some(found) = crate::agent::which("rsrs") {
        return Some(found);
    }
    for cand in rsrs_candidates() {
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn rsrs_candidates() -> Vec<PathBuf> {
    let names: [&str; 2] = ["rsrs.cmd", "rsrs"];
    let mut out: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    {
        let bases = ["APPDATA", "LOCALAPPDATA", "ProgramFiles", "ProgramW6432"];
        for var in bases {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                for name in names {
                    // %APPDATA%\npm\rsrs.cmd
                    out.push(base.join("npm").join(name));
                    // npm 装在 nodejs 目录时的全局前缀
                    out.push(base.join("nodejs").join(name));
                }
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            for name in names {
                out.push(PathBuf::from(&local).join("pnpm").join(name));
            }
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            for name in names {
                out.push(
                    PathBuf::from(&home)
                        .join("AppData")
                        .join("Local")
                        .join("pnpm")
                        .join(name),
                );
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            for name in names {
                out.push(home.join(".local").join("bin").join(name));
                out.push(home.join(".local").join("share").join("pnpm").join(name));
                out.push(home.join(".npm-global").join("bin").join(name));
            }
        }
        for base in ["/usr/local/bin", "/opt/homebrew/bin"] {
            for name in names {
                out.push(PathBuf::from(base).join(name));
            }
        }
    }

    out
}

fn find_pm() -> Option<PackageManager> {
    for name in ["npm", "pnpm"] {
        if let Some(bin) = crate::agent::which(name) {
            if let Ok((0, ..)) = run_capture(&bin, &["--version"], Duration::from_secs(20)) {
                return Some(PackageManager {
                    name: name.to_string(),
                    bin,
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 安装 / 更新
// ---------------------------------------------------------------------------

/// 安装或更新到最新版。`rsrs_cli_install` 命令直接调用它。
pub fn install(app: &AppHandle, cfg: &RsrsCliConfig) -> Result<CliStatus, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("rsrs 安装/更新已在进行中，请等待当前任务结束。".into());
    }
    let _guard = RunningGuard;

    let before = status(cfg);
    let pm = find_pm().ok_or_else(|| {
        "未找到 npm / pnpm，无法自动安装 rsrs。\n\
         请先安装 Node.js（https://nodejs.org），或手动执行：npm i -g rsrs-cli"
            .to_string()
    })?;

    emit(
        app,
        Progress {
            stage: "准备".into(),
            progress: 5,
            message: format!(
                "{} 全局安装 rsrs（{}）…",
                pm.name,
                if before.installed {
                    "更新到最新版"
                } else {
                    "首次安装"
                }
            ),
            log: None,
            done: false,
            ok: true,
            version: before.version.clone(),
        },
    );

    let primary = package_spec(&cfg.package);
    // 已装的是旧包名时直接升级旧包，避免 `rsrs-cli` 与 `@rsrsai/cli` 同装抢同一个命令
    let target = installed_package(&pm, cfg);
    let primary = match target.as_deref() {
        Some(name) => package_spec(name),
        None => primary,
    };
    let mut result = run_install(app, &pm, &primary);

    // 短名包在部分 registry 镜像上可能尚未同步：回退到旧包名（同仓库、同命令，不能同装）
    if let Err(err) = result.as_ref() {
        let fallback = package_spec(&cfg.fallback_package);
        if is_missing_package(err) && !cfg.fallback_package.trim().is_empty() {
            emit(
                app,
                Progress {
                    stage: "回退".into(),
                    progress: 18,
                    message: format!("{primary} 不可用，改用 {fallback} 重试…"),
                    log: Some(tail(err, 160)),
                    done: false,
                    ok: true,
                    version: None,
                },
            );
            result = run_install(app, &pm, &fallback);
        }
    }
    if let Err(err) = result {
        emit(
            app,
            Progress {
                stage: "失败".into(),
                progress: 100,
                message: err.clone(),
                log: None,
                done: true,
                ok: false,
                version: None,
            },
        );
        return Err(err);
    }

    emit(
        app,
        Progress {
            stage: "校验".into(),
            progress: 92,
            message: "安装完成，正在校验 rsrs 命令…".into(),
            log: None,
            done: false,
            ok: true,
            version: None,
        },
    );
    std::thread::sleep(Duration::from_millis(500));
    let after = status(cfg);
    if !after.installed {
        let msg = "安装命令已结束，但仍检测不到 rsrs 命令。\n\
                   请重开终端 / 重启 memTools 后再试，并确认 npm 全局 bin 目录在 PATH 中。"
            .to_string();
        emit(
            app,
            Progress {
                stage: "失败".into(),
                progress: 100,
                message: msg.clone(),
                log: None,
                done: true,
                ok: false,
                version: None,
            },
        );
        return Err(msg);
    }

    let version_text = after
        .version
        .as_deref()
        .map(|v| format!("v{v}"))
        .unwrap_or_else(|| "（版本未知）".into());
    emit(
        app,
        Progress {
            stage: "完成".into(),
            progress: 100,
            message: format!("rsrs {version_text} 已就绪"),
            log: None,
            done: true,
            ok: true,
            version: after.version.clone(),
        },
    );
    Ok(after)
}

/// 查询 registry 最新版本（供「检查更新」按钮）。同步阻塞。
pub fn check_latest(cfg: &RsrsCliConfig) -> Result<UpdateCheck, String> {
    let pm = find_pm().ok_or_else(|| {
        "未找到 npm / pnpm，无法检查 rsrs 新版本。\n请先安装 Node.js（https://nodejs.org）。"
            .to_string()
    })?;
    let current = status(cfg);

    // 已装旧包名时优先查同一个包，避免把「旧包可升级」误报成「未安装」
    let mut package = installed_package(&pm, cfg)
        .unwrap_or_else(|| cfg.package.trim().to_string());
    let mut latest = query_latest(&pm, &package);
    if latest.is_none() {
        for alt in [cfg.package.trim(), cfg.fallback_package.trim()] {
            if alt.is_empty() || alt == package {
                continue;
            }
            if let Some(v) = query_latest(&pm, alt) {
                package = alt.to_string();
                latest = Some(v);
                break;
            }
        }
    }
    let latest = latest.ok_or_else(|| {
        format!(
            "无法查询 {package} 的版本：请检查网络 / npm registry 配置。\n\
             也可在终端执行：{} view {package} version",
            pm.name
        )
    })?;

    let has_update = match current.version.as_deref() {
        Some(v) => crate::updater::version_gt(&latest, v),
        None => true,
    };
    Ok(UpdateCheck {
        installed: current.version,
        has_update,
        latest,
        package,
        package_manager: pm.name,
    })
}

fn package_spec(raw: &str) -> String {
    let name = raw.trim();
    if name.ends_with("@latest") {
        name.to_string()
    } else {
        format!("{name}@latest")
    }
}

fn query_latest(pm: &PackageManager, package: &str) -> Option<String> {
    let (code, out, _err) = run_capture(
        &pm.bin,
        &["view", package, "version"],
        Duration::from_secs(45),
    )
    .ok()?;
    if code != 0 {
        return None;
    }
    parse_version(&out)
}

/// 已全局安装的 rsrs 包名（优先 `rsrs-cli`，其次 `@rsrsai/cli`）。
///
/// 两个包同仓库、同 `rsrs` 命令：升级时沿用原来的包名，避免同装抢命令。
fn installed_package(pm: &PackageManager, cfg: &RsrsCliConfig) -> Option<String> {
    let (_code, out, _err) = run_capture(
        &pm.bin,
        &["ls", "-g", "--depth=0", "--json"],
        Duration::from_secs(30),
    )
    .ok()?;
    pick_installed(&out, cfg.package.trim(), cfg.fallback_package.trim())
}

fn pick_installed(json: &str, primary: &str, fallback: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let deps = value.get("dependencies")?.as_object()?;
    for name in [primary, fallback] {
        if !name.is_empty() && deps.contains_key(name) {
            return Some(name.to_string());
        }
    }
    deps.keys()
        .find(|name| name.contains("rsrs") || name.contains("1memory"))
        .cloned()
}

fn is_missing_package(err: &str) -> bool {
    let low = err.to_lowercase();
    low.contains("e404")
        || low.contains("404 not found")
        || low.contains("is not in this registry")
        || low.contains("no matching version")
}

/// 执行一次 `npm install -g <spec>`，逐行把输出转成进度事件。
fn run_install(app: &AppHandle, pm: &PackageManager, spec: &str) -> Result<(), String> {
    let mut cmd = Command::new(&pm.bin);
    cmd.args(["install", "-g", "--no-fund", "--no-audit", spec])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW：不弹控制台黑窗
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法启动 {}：{e}", pm.name))?;

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);

    let deadline = Instant::now() + INSTALL_TIMEOUT;
    let mut progress: u16 = 20;
    let mut output = String::new();

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                // 只保留尾部，避免内存无上限；错误判定用尾部足够
                output.push_str(line);
                output.push('\n');
                if output.len() > 6000 {
                    let keep_from = output.len() - 4000;
                    output = output[keep_from..].to_string();
                }
                progress = (progress + 1).min(86);
                emit(
                    app,
                    Progress {
                        stage: "安装中".into(),
                        progress: progress as u8,
                        message: format!("{} 正在安装 {spec}", pm.name),
                        log: Some(tail(line, 220)),
                        done: false,
                        ok: true,
                        version: None,
                    },
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {}
        }

        match child.try_wait() {
            Ok(Some(state)) => {
                // 收干剩余输出（npm 可能在退出前才刷完日志）
                while let Ok(line) = rx.try_recv() {
                    let line = line.trim();
                    if !line.is_empty() {
                        output.push_str(line);
                        output.push('\n');
                    }
                }
                if state.success() {
                    return Ok(());
                }
                let code = state.code().unwrap_or(-1);
                return Err(format!(
                    "{} 安装 {spec} 失败（退出码 {code}）：{}",
                    pm.name,
                    tail(&output, 600)
                ));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("等待 {} 进程失败：{e}", pm.name));
            }
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{} 安装超时（{} 分钟）。请检查网络或代理后重试。",
                pm.name,
                INSTALL_TIMEOUT.as_secs() / 60
            ));
        }
    }
}

fn emit(app: &AppHandle, p: Progress) {
    let _ = app.emit(EVENT, p);
}

// ---------------------------------------------------------------------------
// 工具：带超时的子进程捕获
// ---------------------------------------------------------------------------

pub(crate) fn run_capture(bin: &std::path::Path, args: &[&str], timeout: Duration) -> Result<(i32, String, String), String> {
    run_capture_env(bin, args, timeout, &[])
}

/// 同 [`run_capture`]，但先移除指定环境变量。
///
/// 开机自启需要让 rsrs 自行拉起常驻 runtime：沙盒可能注入
/// `RSRS_CLIENT_ONLY` / `ONEMEMORY_NO_AUTOSTART`，两者都会阻止启动。
pub(crate) fn run_capture_env(
    bin: &std::path::Path,
    args: &[&str],
    timeout: Duration,
    remove_env: &[&str],
) -> Result<(i32, String, String), String> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in remove_env {
        cmd.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = cmd.spawn().map_err(|e| format!("无法启动 {}：{e}", bin.display()))?;

    let (tx_out, rx_out) = std::sync::mpsc::channel::<String>();
    let (tx_err, rx_err) = std::sync::mpsc::channel::<String>();
    if let Some(mut out) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut buf = String::new();
            use std::io::Read;
            let _ = out.read_to_string(&mut buf);
            let _ = tx_out.send(buf);
        });
    }
    if let Some(mut err) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut buf = String::new();
            use std::io::Read;
            let _ = err.read_to_string(&mut buf);
            let _ = tx_err.send(buf);
        });
    }

    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(state)) => break state.code().unwrap_or(-1),
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                return Err(format!("等待 {} 退出失败：{e}", bin.display()));
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} 执行超时（{}s）", bin.display(), timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let stdout = rx_out.recv_timeout(Duration::from_secs(3)).unwrap_or_default();
    let stderr = rx_err.recv_timeout(Duration::from_secs(3)).unwrap_or_default();
    Ok((code, stdout, stderr))
}

// ---------------------------------------------------------------------------
// 纯函数（可单测）
// ---------------------------------------------------------------------------

/// 从命令输出里抽版本号：`rsrs 1.0.16` / `1.0.16` / `1.0.16-dev.1` 均可。
fn parse_version(raw: &str) -> Option<String> {
    raw.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .map(|t| t.trim().trim_start_matches('v'))
        .find(|t| looks_like_version(t))
        .map(|t| t.to_string())
}

fn looks_like_version(token: &str) -> bool {
    let mut parts = token.split('.');
    let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let starts_digit = |s: &str| s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
    digits(a) && digits(b) && starts_digit(c)
}

/// 取末尾 N 个字符（错误回显用）
fn tail(s: &str, n: usize) -> String {
    let t = s.trim();
    let count = t.chars().count();
    if count <= n {
        return t.to_string();
    }
    t.chars().skip(count - n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("rsrs 1.0.16"), Some("1.0.16".into()));
        assert_eq!(parse_version("v1.0.15\r\n"), Some("1.0.15".into()));
        assert_eq!(parse_version("1.0.16-dev.3"), Some("1.0.16-dev.3".into()));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version("node v22.22.2"), Some("22.22.2".into()));
    }

    #[test]
    fn package_spec_appends_latest() {
        assert_eq!(package_spec("rsrs-cli"), "rsrs-cli@latest");
        assert_eq!(package_spec(" rsrs-cli@latest "), "rsrs-cli@latest");
        assert_eq!(package_spec("@rsrsai/cli"), "@rsrsai/cli@latest");
    }

    #[test]
    fn missing_package_detection() {
        assert!(is_missing_package(
            "npm error 404 Not Found - GET https://registry.npmjs.org/rsrs-cli - Not found"
        ));
        assert!(is_missing_package("npm error code E404"));
        assert!(!is_missing_package("npm error network request failed"));
    }

    #[test]
    fn tail_keeps_last_chars() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 10), "abc");
    }

    #[test]
    fn picks_the_installed_package_name() {
        let long = r#"{"dependencies":{"@rsrsai/cli":{"version":"1.0.16"}}}"#;
        assert_eq!(
            pick_installed(long, "rsrs-cli", "@rsrsai/cli"),
            Some("@rsrsai/cli".into())
        );
        let short = r#"{"dependencies":{"rsrs-cli":{"version":"1.0.16"}}}"#;
        assert_eq!(
            pick_installed(short, "rsrs-cli", "@rsrsai/cli"),
            Some("rsrs-cli".into())
        );
        let none = r#"{"dependencies":{}}"#;
        assert_eq!(pick_installed(none, "rsrs-cli", "@rsrsai/cli"), None);
        assert_eq!(pick_installed("not json", "rsrs-cli", "@rsrsai/cli"), None);
    }
}
