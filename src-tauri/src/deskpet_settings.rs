//! DeskPet 状态推送的界面开关。
//!
//! `config.toml` 里的 `[deskpet] enabled` 是默认值；设置面板的开关写入
//! `{app_config_dir}/deskpet.json`，叠加在 `config.toml` 之上并立即生效。
//! 这样普通用户只需要在界面开关，`interval_secs` / `control_file` 等高级参数
//! 仍由 `config.toml` 管理。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::DeskpetConfig;

/// 持久化到 `deskpet.json` 的用户选择。
///
/// `None` 表示跟随 `config.toml`；只有用户点过开关才写显式布尔值。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct StoredChoice {
    enabled: Option<bool>,
}

/// 线程安全的开关读写。
pub struct DeskpetSettings {
    path: PathBuf,
    inner: Mutex<StoredChoice>,
}

impl DeskpetSettings {
    /// 从 `deskpet.json` 载入；缺失 / 损坏时回退为「跟随 config.toml」。
    pub fn load(path: PathBuf) -> Self {
        let choice = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<StoredChoice>(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(choice),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn choice(&self) -> StoredChoice {
        self.inner
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// 生效开关 = `deskpet.json` 的显式选择，否则跟随 `config.toml`。
    pub fn is_enabled(&self, base: &DeskpetConfig) -> bool {
        self.choice().enabled.unwrap_or(base.enabled)
    }

    /// 保存开关并返回生效值；写盘失败时内存状态不变。
    pub fn set_enabled(&self, enabled: bool) -> Result<bool, String> {
        let choice = StoredChoice {
            enabled: Some(enabled),
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("创建设置目录失败：{e}（{}）", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(&choice)
            .map_err(|e| format!("序列化 DeskPet 设置失败：{e}"))?;
        std::fs::write(&self.path, json)
            .map_err(|e| format!("写入 DeskPet 设置失败：{e}（{}）", self.path.display()))?;
        match self.inner.lock() {
            Ok(mut guard) => *guard = choice,
            Err(poisoned) => *poisoned.into_inner() = choice,
        }
        Ok(enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_follows_config_default() {
        let dir = std::env::temp_dir().join(format!(
            "memtools-deskpet-settings-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = DeskpetSettings::load(dir.join("deskpet.json"));
        let mut base = DeskpetConfig::default();
        assert!(!settings.is_enabled(&base));
        base.enabled = true;
        assert!(settings.is_enabled(&base), "未写入时应跟随 config.toml");
    }

    #[test]
    fn explicit_choice_survives_reload_and_overrides_config() {
        let dir =
            std::env::temp_dir().join(format!("memtools-deskpet-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("deskpet.json");
        let settings = DeskpetSettings::load(path.clone());
        assert_eq!(settings.set_enabled(true).unwrap(), true);
        assert!(path.exists(), "应写入 deskpet.json");

        let reloaded = DeskpetSettings::load(path.clone());
        let mut base = DeskpetConfig::default();
        assert!(reloaded.is_enabled(&base));
        base.enabled = false;
        assert!(reloaded.is_enabled(&base), "界面开关应覆盖 config.toml");

        reloaded.set_enabled(false).unwrap();
        assert!(!DeskpetSettings::load(path).is_enabled(&base));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back_without_panicking() {
        let dir = std::env::temp_dir().join(format!(
            "memtools-deskpet-settings-corrupt-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deskpet.json");
        std::fs::write(&path, "{not json").unwrap();
        let settings = DeskpetSettings::load(path);
        assert!(!settings.is_enabled(&DeskpetConfig::default()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
