//! 知识库运行时设置（P13）
//!
//! `config.toml` 提供默认值与硬限制（单文件上限、文件数、超时、模型）；
//! AI 窗口里选的根目录、右键菜单开关存到 `{app_config_dir}/knowledge_base.json`，
//! 叠加在 `config.toml` 之上并立即生效。
//!
//! 空字符串 / `None` 一律表示「跟随 config.toml」，只有显式选择才覆盖。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::KnowledgeBaseConfig;

/// 叠加在 `config.toml` 之上的用户选择
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KbChoice {
    /// 知识库根目录（空 = 跟随 config.toml）
    pub root: String,
    /// 绑定的 agent：opencode | qoder | custom（空 = 跟随 config.toml）
    pub agent_kind: String,
    /// agent 可执行文件（空 = 自动探测）
    pub agent_bin: String,
    /// custom：参数模板（空 = 跟随 config.toml）
    pub agent_args: Vec<String>,
    /// custom：附加环境变量（KEY=VALUE，空 = 跟随 config.toml）
    pub agent_env: Vec<String>,
    /// custom：输出解析方式（空 = 跟随 config.toml）
    pub agent_output: String,
    /// 右键菜单开关（None = 跟随 config.toml）
    pub context_menu: Option<bool>,
    /// opencode 模型（空 = 跟随 config.toml）
    pub agent_model: String,
    /// 是否启用（None = 跟随 config.toml）
    pub enabled: Option<bool>,
}

impl KbChoice {
    pub fn normalized(mut self) -> Self {
        self.root = self.root.trim().to_string();
        self.agent_kind = self.agent_kind.trim().to_lowercase();
        self.agent_bin = self.agent_bin.trim().to_string();
        self.agent_model = self.agent_model.trim().to_string();
        self.agent_output = self.agent_output.trim().to_lowercase();
        self.agent_args = self
            .agent_args
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        self.agent_env = self
            .agent_env
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        self
    }

    pub fn is_default(&self) -> bool {
        self.root.is_empty()
            && self.agent_kind.is_empty()
            && self.agent_bin.is_empty()
            && self.agent_args.is_empty()
            && self.agent_env.is_empty()
            && self.agent_output.is_empty()
            && self.context_menu.is_none()
            && self.agent_model.is_empty()
            && self.enabled.is_none()
    }
}

/// 把用户选择叠加到 `config.toml` 的知识库配置上（纯函数，便于单测）
pub fn apply(base: &KnowledgeBaseConfig, choice: &KbChoice) -> KnowledgeBaseConfig {
    let mut cfg = base.clone();
    if !choice.root.trim().is_empty() {
        cfg.root = choice.root.trim().to_string();
    }
    if !choice.agent_kind.trim().is_empty() {
        cfg.agent_kind = choice.agent_kind.trim().to_lowercase();
    }
    if !choice.agent_bin.trim().is_empty() {
        cfg.agent_bin = choice.agent_bin.trim().to_string();
    }
    if let Some(v) = choice.context_menu {
        cfg.context_menu = v;
    }
    if !choice.agent_model.trim().is_empty() {
        cfg.agent_model = choice.agent_model.trim().to_string();
    }
    if !choice.agent_args.is_empty() {
        cfg.agent_args = choice.agent_args.clone();
    }
    if !choice.agent_env.is_empty() {
        cfg.agent_env = choice.agent_env.clone();
    }
    if !choice.agent_output.trim().is_empty() {
        cfg.agent_output = choice.agent_output.trim().to_lowercase();
    }
    if let Some(v) = choice.enabled {
        cfg.enabled = v;
    }
    cfg
}

/// 选择持久化 + 线程安全读写
pub struct KbSettings {
    path: PathBuf,
    inner: Mutex<KbChoice>,
}

impl KbSettings {
    pub fn load(path: PathBuf) -> Self {
        let choice = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<KbChoice>(&s).ok())
            .unwrap_or_default()
            .normalized();
        Self {
            path,
            inner: Mutex::new(choice),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> KbChoice {
        self.inner
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    pub fn set(&self, choice: KbChoice) -> Result<KbChoice, String> {
        let choice = choice.normalized();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("创建设置目录失败：{e}（{}）", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(&choice)
            .map_err(|e| format!("序列化知识库设置失败：{e}"))?;
        std::fs::write(&self.path, json)
            .map_err(|e| format!("写入设置失败：{e}（{}）", self.path.display()))?;
        match self.inner.lock() {
            Ok(mut g) => *g = choice.clone(),
            Err(e) => *e.into_inner() = choice.clone(),
        }
        Ok(choice)
    }

    /// 只改一个字段，其余保持不变
    pub fn patch<F>(&self, f: F) -> Result<KbChoice, String>
    where
        F: FnOnce(&mut KbChoice),
    {
        let mut choice = self.get();
        f(&mut choice);
        self.set(choice)
    }

    pub fn reset(&self) -> Result<KbChoice, String> {
        self.set(KbChoice::default())
    }

    /// 生效配置 = config.toml + 用户选择
    pub fn effective(&self, base: &KnowledgeBaseConfig) -> KnowledgeBaseConfig {
        apply(base, &self.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> KnowledgeBaseConfig {
        KnowledgeBaseConfig {
            root: r"D:\kb".into(),
            context_menu: true,
            ..Default::default()
        }
    }

    #[test]
    fn default_choice_keeps_config() {
        let got = apply(&base(), &KbChoice::default());
        assert_eq!(got.root, r"D:\kb");
        assert!(got.context_menu);
    }

    #[test]
    fn root_and_toggles_override() {
        let got = apply(
            &base(),
            &KbChoice {
                root: "  D:\\notes  ".into(),
                agent_kind: " Qoder ".into(),
                agent_bin: "  D:\\qodercli.exe  ".into(),
                context_menu: Some(false),
                agent_model: "opencode-go/deepseek-v4.1-flash".into(),
                enabled: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(got.root, r"D:\notes");
        assert_eq!(got.agent_kind, "qoder");
        assert_eq!(got.agent_bin, r"D:\qodercli.exe");
        assert!(!got.context_menu);
        assert_eq!(got.agent_model, "opencode-go/deepseek-v4.1-flash");
        assert!(!got.enabled);
    }

    #[test]
    fn agent_binding_defaults_to_config() {
        let got = apply(&base(), &KbChoice::default());
        assert_eq!(got.agent_kind, "opencode");
        assert!(got.agent_bin.is_empty(), "空值不应覆盖 config.toml");
    }

    #[test]
    fn empty_strings_do_not_override() {
        let got = apply(&base(), &KbChoice::normalized_blank());
        assert_eq!(got.root, r"D:\kb");
        assert!(got.agent_model.is_empty());
    }

    #[test]
    fn settings_roundtrip_and_patch() {
        let dir = std::env::temp_dir().join(format!("memtools-kb-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("knowledge_base.json");
        let s = KbSettings::load(path.clone());
        assert!(s.get().is_default());

        s.set(KbChoice {
            root: r"D:\kb".into(),
            ..Default::default()
        })
        .expect("保存应成功");
        s.patch(|c| c.context_menu = Some(false))
            .expect("patch 应成功");

        let again = KbSettings::load(path);
        assert_eq!(again.get().root, r"D:\kb");
        assert_eq!(again.get().context_menu, Some(false));

        again.reset().expect("重置应成功");
        assert!(KbSettings::load(dir.join("knowledge_base.json"))
            .get()
            .is_default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    impl KbChoice {
        fn normalized_blank() -> Self {
            KbChoice {
                root: "  ".into(),
                agent_model: " ".into(),
                ..Default::default()
            }
            .normalized()
        }
    }
}
