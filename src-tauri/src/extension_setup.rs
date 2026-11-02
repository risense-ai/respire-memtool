//! 导出本机私有扩展：公开内嵌资源不含令牌，配对信息仅写入用户配置目录。
use std::path::{Path, PathBuf};
use std::process::Command;

const FILES: &[(&str, &[u8])] = &[
    (
        "manifest.json",
        include_bytes!("../../extension/manifest.json"),
    ),
    (
        "background.js",
        include_bytes!("../../extension/background.js"),
    ),
    ("settings.js", include_bytes!("../../extension/settings.js")),
    ("options.js", include_bytes!("../../extension/options.js")),
    (
        "options.html",
        include_bytes!("../../extension/options.html"),
    ),
    ("icon128.png", include_bytes!("../../extension/icon128.png")),
];

/// 生成可由 Chrome 加载的本机私有目录；不得把该目录作为公开发布包分发。
pub fn prepare(dir: &Path, port: u16, token: &str) -> Result<PathBuf, String> {
    if port == 0 || token.trim().is_empty() {
        return Err("扩展端点尚未就绪，请先检查扩展端点是否启用且端口可用。".into());
    }
    // 1. 写入公开扩展资源，保持与当前桌面版本一致。
    std::fs::create_dir_all(dir).map_err(|e| format!("创建本机扩展目录失败：{e}"))?;
    for (name, bytes) in FILES {
        std::fs::write(dir.join(name), bytes)
            .map_err(|e| format!("准备扩展文件 {name} 失败：{e}"))?;
    }
    // 2. 在用户配置目录中写入私有配对信息，源码和可执行文件不嵌入真实令牌。
    let pairing = serde_json::json!({ "port": port, "token": token.trim() });
    std::fs::write(dir.join("pairing.json"), pairing.to_string())
        .map_err(|e| format!("写入本机配对信息失败：{e}"))?;
    Ok(dir.to_path_buf())
}

/// 浏览器候选：可执行文件相对路径、扩展管理页、用户数据目录（相对 LOCALAPPDATA）
#[cfg(windows)]
const BROWSERS: &[(&str, &str, &str)] = &[
    (
        "Google/Chrome/Application/chrome.exe",
        "chrome://extensions/",
        "Google/Chrome/User Data",
    ),
    (
        "Microsoft/Edge/Application/msedge.exe",
        "edge://extensions/",
        "Microsoft/Edge/User Data",
    ),
];

/// 已知安装位置里的浏览器可执行文件（不查 PATH，避免命中同名程序）
#[cfg(windows)]
fn browser_exe(rel: &str) -> Option<PathBuf> {
    ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(rel))
        .find(|path| path.is_file())
}

/// Preferences 里保存路径时反斜杠可能被转义，两种写法都要匹配
pub fn path_markers(dir: &Path) -> [String; 2] {
    let plain = dir.to_string_lossy().to_lowercase();
    let escaped = plain.replace('\\', "\\\\");
    [plain, escaped]
}

/// 该浏览器的任意用户配置里是否已加载此目录的扩展
#[cfg(windows)]
fn browser_has_extension(dir: &Path, user_data: &str) -> bool {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return false;
    };
    let root = PathBuf::from(local).join(user_data);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return false;
    };
    let [plain, escaped] = path_markers(dir);
    for entry in entries.flatten() {
        let pref = entry.path().join("Preferences");
        let Ok(raw) = std::fs::read_to_string(&pref) else {
            continue;
        };
        let lower = raw.to_lowercase();
        if lower.contains(&plain) || lower.contains(&escaped) {
            return true;
        }
    }
    false
}

/// 只打开浏览器的安装管理页和目录，加载/重载确认仍由浏览器与用户完成。
///
/// 优先打开**已加载过该扩展**的浏览器（Chrome / Edge），都没加载过才退回 Chrome、再退 Edge。
pub fn open_install_ui(dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut opened: Option<PathBuf> = None;
        for (rel, url, user_data) in BROWSERS {
            let Some(exe) = browser_exe(rel) else { continue };
            if browser_has_extension(dir, user_data) {
                Command::new(&exe)
                    .arg(url)
                    .spawn()
                    .map_err(|e| format!("打开浏览器扩展管理页失败：{e}"))?;
                opened = Some(exe);
                break;
            }
        }
        if opened.is_none() {
            for (rel, url, _) in BROWSERS {
                let Some(exe) = browser_exe(rel) else { continue };
                Command::new(&exe)
                    .arg(url)
                    .spawn()
                    .map_err(|e| format!("打开浏览器扩展管理页失败：{e}"))?;
                opened = Some(exe);
                break;
            }
        }
        let exe = opened.ok_or("未检测到 Chrome 或 Edge，请手动打开浏览器的扩展管理页。")?;
        let _ = exe;
        // 同时打开已准备好的目录，便于在加载对话框定位。
        let explorer =
            PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()))
                .join("explorer.exe");
        Command::new(explorer)
            .arg(dir)
            .spawn()
            .map_err(|e| format!("扩展已准备好，打开目录失败：{e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Err("扩展已准备好，请手动打开浏览器扩展管理页加载此目录。".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_export_contains_pairing_and_all_loadable_assets() {
        let dir = std::env::temp_dir().join(format!("memtools-extension-{}", uuid::Uuid::new_v4()));
        prepare(&dir, 18765, "test-only-fixture").unwrap();
        let pairing: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("pairing.json")).unwrap()).unwrap();
        assert_eq!(pairing["port"], 18765);
        assert_eq!(pairing["token"], "test-only-fixture");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        assert!(dir
            .join(manifest["background"]["service_worker"].as_str().unwrap())
            .is_file());
        assert!(dir.join("settings.js").is_file());
        assert!(dir
            .join(manifest["options_page"].as_str().unwrap())
            .is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unready_endpoint_does_not_export_a_broken_pairing() {
        let dir = std::env::temp_dir().join(format!("memtools-extension-{}", uuid::Uuid::new_v4()));
        assert!(prepare(&dir, 0, "fixture").is_err());
        assert!(prepare(&dir, 17650, "").is_err());
        assert!(!dir.exists());
    }

    #[test]
    fn path_markers_cover_plain_and_escaped_forms() {
        let dir = Path::new(r"C:\Users\u\AppData\Roaming\com.memtools.desktop\chrome-extension");
        let [plain, escaped] = path_markers(dir);
        assert!(plain.contains("com.memtools.desktop\\chrome-extension"));
        assert!(!plain.contains(r"\\"));
        assert!(escaped.contains("com.memtools.desktop\\\\chrome-extension"));
        assert_eq!(escaped.matches('\\').count(), plain.matches('\\').count() * 2);
    }
}
