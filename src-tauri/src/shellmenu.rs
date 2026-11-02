//! Windows 资源管理器右键菜单（P13）
//!
//! 只写 `HKEY_CURRENT_USER`，不需要管理员权限：
//!
//! ```text
//! HKCU\Software\Classes\*\shell\memToolsKb                 文件
//! HKCU\Software\Classes\Directory\shell\memToolsKb         文件夹
//! HKCU\Software\Classes\Directory\Background\shell\memToolsKb  文件夹空白处
//!   (默认) = 添加到 memTools 知识库
//!   Icon   = <exe>
//!   MultiSelectModel = Document        （多选时逐个调用）
//!   command\(默认) = "<exe>" --kb-add "%1"
//! ```
//!
//! 注意：Windows 11 把经典右键项折叠进「显示更多选项」（Shift+F10），
//! 这是系统行为，不做 MSIX 打包就无法进一级菜单。

use std::path::Path;

/// 注册表里的菜单键名（卸载时按同名删除）
pub const KEY_NAME: &str = "memToolsKb";
/// 菜单显示名
pub const MENU_LABEL: &str = "添加到 memTools 知识库";

/// 需要注册的三处位置（相对 HKCU\Software\Classes）
pub fn key_paths_named(name: &str) -> [String; 3] {
    [
        format!("Software\\Classes\\*\\shell\\{name}"),
        format!("Software\\Classes\\Directory\\shell\\{name}"),
        format!("Software\\Classes\\Directory\\Background\\shell\\{name}"),
    ]
}

/// 默认键名对应的三处注册位置
pub fn key_paths() -> [String; 3] {
    key_paths_named(KEY_NAME)
}

/// 每条注册项的命令行（`%1` 为选中项，`%V` 为当前文件夹）
pub fn command_line(exe: &Path, placeholder: &str) -> String {
    format!("\"{}\" --kb-add \"{}\"", exe.display(), placeholder)
}

/// 一处注册项的占位符（Background 用 `%V`）
pub fn placeholder_for(index: usize) -> &'static str {
    if index == 2 {
        "%V"
    } else {
        "%1"
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    fn hkcu() -> Result<RegKey, String> {
        // 注意：这里必须打开 HKCU 本身，key_paths() 里的路径是相对 HKCU 的；
        // 若先打开 Software\Classes 再传完整路径，会写成 Software\Classes\Software\Classes\...
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("", winreg::enums::KEY_READ | winreg::enums::KEY_WRITE)
            .map_err(|e| format!("无法打开 HKEY_CURRENT_USER：{e}"))
    }

    pub fn install(exe: &Path) -> Result<(), String> {
        install_named(KEY_NAME, exe)
    }

    pub fn install_named(name: &str, exe: &Path) -> Result<(), String> {
        let classes = hkcu()?;
        for (index, path) in key_paths_named(name).iter().enumerate() {
            let (key, _) = classes
                .create_subkey(path)
                .map_err(|e| format!("创建右键菜单项失败（{path}）：{e}"))?;
            key.set_value("", &MENU_LABEL)
                .map_err(|e| format!("写入菜单名失败（{path}）：{e}"))?;
            key.set_value("Icon", &exe.display().to_string())
                .map_err(|e| format!("写入菜单图标失败（{path}）：{e}"))?;
            // 多选时逐个调用：单例插件会把每次的参数转发到同一个进程
            key.set_value("MultiSelectModel", &"Document")
                .map_err(|e| format!("写入多选模式失败（{path}）：{e}"))?;
            let (cmd_key, _) = key
                .create_subkey("command")
                .map_err(|e| format!("创建 command 子键失败（{path}）：{e}"))?;
            let line = command_line(exe, placeholder_for(index));
            cmd_key
                .set_value("", &line)
                .map_err(|e| format!("写入命令行失败（{path}）：{e}"))?;
        }
        Ok(())
    }

    pub fn remove() -> Result<(), String> {
        remove_named(KEY_NAME)
    }

    pub fn remove_named(name: &str) -> Result<(), String> {
        let classes = hkcu()?;
        let mut errors: Vec<String> = Vec::new();
        for path in key_paths_named(name) {
            match classes.delete_subkey_all(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => errors.push(format!("{path}: {e}")),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("删除右键菜单项失败：{}", errors.join("；")))
        }
    }

    pub fn status() -> bool {
        status_named(KEY_NAME)
    }

    pub fn status_named(name: &str) -> bool {
        let Ok(classes) = hkcu() else {
            return false;
        };
        key_paths_named(name).iter().all(|path| {
            classes
                .open_subkey(path)
                .and_then(|k| k.open_subkey("command"))
                .map(|c| c.get_value::<String, _>("").is_ok())
                .unwrap_or(false)
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn install(_exe: &Path) -> Result<(), String> {
        Err("只有 Windows 支持资源管理器右键菜单".into())
    }

    pub fn install_named(_name: &str, _exe: &Path) -> Result<(), String> {
        Err("只有 Windows 支持资源管理器右键菜单".into())
    }

    pub fn remove() -> Result<(), String> {
        Ok(())
    }

    pub fn remove_named(_name: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn status() -> bool {
        false
    }

    pub fn status_named(_name: &str) -> bool {
        false
    }
}

pub use imp::{install, install_named, remove, remove_named, status, status_named};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_exe_and_placeholder() {
        let exe = Path::new(r"C:\Program Files\memTools\memtools.exe");
        assert_eq!(
            command_line(exe, "%1"),
            r#""C:\Program Files\memTools\memtools.exe" --kb-add "%1""#
        );
    }

    #[test]
    fn key_paths_cover_file_folder_and_background() {
        let paths = key_paths();
        assert_eq!(paths.len(), 3);
        assert!(paths[0].contains("\\*\\shell\\"));
        assert!(paths[1].contains("Directory\\shell"));
        assert!(paths[2].contains("Background"));
        assert_eq!(placeholder_for(0), "%1");
        assert_eq!(placeholder_for(2), "%V");
    }

    /// 真实写一次 HKCU（用临时键名），确认注册/查询/删除闭环
    #[cfg(windows)]
    #[test]
    fn registry_roundtrip_with_temp_name() {
        let name = "memToolsKbTest";
        let exe = Path::new(r"C:\Windows\System32\notepad.exe");

        let _ = remove_named(name);
        assert!(!status_named(name));
        install_named(name, exe).expect("注册应成功");
        assert!(status_named(name));

        // 命令行走 %1 / %V，且 exe 带引号
        let paths = key_paths_named(name);
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let classes = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Classes")
            .expect("应能读取 Classes");
        for (i, path) in paths.iter().enumerate() {
            let relative = path
                .strip_prefix(r"Software\Classes")
                .expect("路径应相对 HKCU")
                .trim_start_matches('\\');
            let cmd: String = classes
                .open_subkey(format!("{relative}\\command"))
                .and_then(|k| k.get_value(""))
                .expect("命令行应存在");
            let placeholder = placeholder_for(i);
            assert!(cmd.contains(placeholder), "命令行应含 {placeholder}：{cmd}");
            assert!(cmd.starts_with('"'), "exe 应加引号：{cmd}");
        }

        remove_named(name).expect("删除应成功");
        assert!(!status_named(name));
    }
}
