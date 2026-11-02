//! 知识库 agent 工作区（P13）
//!
//! memTools 调 opencode 时**不使用用户当前项目目录**，而是在应用配置目录下维护一个
//! 独立工作区：
//!
//! ```text
//! {app_config_dir}/agent-workspace/
//!   AGENTS.md                     本工作区规则
//!   opencode.json                 权限白名单（只读资料 + 写知识库 + rsrs 命令）
//!   .opencode/skills/kb-ingest/   入库技能
//!   .opencode/skills/kb-research/ 调研技能
//! ```
//!
//! 这样不碰用户的 `opencode.jsonc`，也不让 agent 把 memTools 仓库当项目。
//! 权限采用「默认全禁 → 逐项放开」：读资料、抓网页、写知识库、跑 `rsrs`，
//! 其余 shell 一律拒绝，降低资料里藏指令的风险。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// 入库技能正文（编译期嵌入，运行时落盘）
const AGENTS_MD: &str = include_str!("../skills/AGENTS.md");
const SKILL_INGEST: &str = include_str!("../skills/kb-ingest/SKILL.md");
const SKILL_RESEARCH: &str = include_str!("../skills/kb-research/SKILL.md");

/// 已就绪的工作区
#[derive(Debug, Clone)]
pub struct Workspace {
    pub dir: PathBuf,
    /// `opencode.json` 路径
    pub config: PathBuf,
}

/// 权限规则里的路径写法：正斜杠、去掉尾部分隔符
pub fn pattern_path(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if s.is_empty() {
        "/".into()
    } else {
        s
    }
}

/// 生成 `opencode.json` 内容（纯函数，便于单测）
pub fn build_config(workspace: &Path, kb_root: Option<&Path>, research: bool) -> Value {
    let ws = pattern_path(workspace);
    let mut permissions: Vec<Value> = vec![
        // 兜底：先全禁，再逐项放开（最后匹配者生效）
        json!({"action": "*", "resource": "*", "effect": "deny"}),
        // 允许读工作区外的资料（原件在知识库目录里）
        json!({"action": "external_directory", "resource": "*", "effect": "allow"}),
        json!({"action": "read", "resource": "*", "effect": "allow"}),
        json!({"action": "glob", "resource": "*", "effect": "allow"}),
        json!({"action": "grep", "resource": "*", "effect": "allow"}),
        // 只允许写工作区与知识库根目录
        json!({"action": "edit", "resource": format!("{ws}/*"), "effect": "allow"}),
        // 只允许加载知识库技能
        json!({"action": "skill", "resource": "kb-ingest", "effect": "allow"}),
        json!({"action": "skill", "resource": "kb-research", "effect": "allow"}),
        // 记忆读写只走 rsrs CLI（改并挂存由 agent 判断）
        json!({"action": "shell", "resource": "rsrs", "effect": "allow"}),
        json!({"action": "shell", "resource": "rsrs *", "effect": "allow"}),
        // 无人值守：不允许反问问句，避免挂起
        json!({"action": "question", "resource": "*", "effect": "deny"}),
    ];
    if let Some(root) = kb_root {
        let kb = pattern_path(root);
        permissions.push(json!({"action": "edit", "resource": format!("{kb}/*"), "effect": "allow"}));
    }
    if research {
        permissions.push(json!({"action": "webfetch", "resource": "*", "effect": "allow"}));
        permissions.push(json!({"action": "websearch", "resource": "*", "effect": "allow"}));
    }
    json!({
        "$schema": "https://opencode.ai/config.json",
        "instructions": ["./AGENTS.md"],
        "permissions": permissions,
    })
}

/// 建好（或刷新）工作区；`kb_root` 变化时会重写权限文件
pub fn ensure(base_dir: &Path, kb_root: Option<&Path>, research: bool) -> Result<Workspace, String> {
    let dir = base_dir.join("agent-workspace");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("无法创建 agent 工作区 {}：{e}", dir.display()))?;
    let skills = dir.join(".opencode").join("skills");
    write_if_changed(&dir.join("AGENTS.md"), AGENTS_MD)?;
    write_if_changed(&skills.join("kb-ingest").join("SKILL.md"), SKILL_INGEST)?;
    write_if_changed(&skills.join("kb-research").join("SKILL.md"), SKILL_RESEARCH)?;

    let config_path = dir.join("opencode.json");
    let desired = build_config(&dir, kb_root, research);
    let text = serde_json::to_string_pretty(&desired)
        .map_err(|e| format!("序列化 agent 配置失败：{e}"))?;
    write_if_changed(&config_path, &text)?;
    Ok(Workspace {
        dir,
        config: config_path,
    })
}

/// 内容相同不落盘（避免每次都改 mtime 触发 opencode 重载）
fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == content {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建目录 {}：{e}", parent.display()))?;
    }
    std::fs::write(path, content)
        .map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "memtools-ws-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pattern_path_uses_forward_slashes() {
        assert_eq!(pattern_path(Path::new(r"D:\kb\notes\")), "D:/kb/notes");
        assert_eq!(pattern_path(Path::new("/")), "/");
    }

    #[test]
    fn config_denies_first_and_allows_narrowly() {
        let ws = Path::new(r"C:\Users\u\AppData\Roaming\com.memtools.desktop\agent-workspace");
        let kb = Path::new(r"D:\kb");
        let cfg = build_config(ws, Some(kb), true);
        let rules = cfg["permissions"].as_array().unwrap();
        assert_eq!(rules[0]["effect"], "deny");
        assert_eq!(rules[0]["resource"], "*");
        assert!(rules
            .iter()
            .any(|r| r["action"] == "edit" && r["resource"] == "D:/kb/*" && r["effect"] == "allow"));
        assert!(rules
            .iter()
            .any(|r| r["action"] == "shell" && r["resource"] == "rsrs *" && r["effect"] == "allow"));
        assert!(rules
            .iter()
            .any(|r| r["action"] == "webfetch" && r["effect"] == "allow"));
    }

    #[test]
    fn research_off_keeps_web_tools_denied() {
        let cfg = build_config(Path::new("/ws"), None, false);
        let rules = cfg["permissions"].as_array().unwrap();
        assert!(!rules.iter().any(|r| r["action"] == "webfetch"));
        assert!(!rules.iter().any(|r| r["action"] == "websearch"));
        assert!(!rules.iter().any(|r| r["action"] == "edit" && r["resource"] == "D:/kb/*"));
    }

    #[test]
    fn ensure_writes_workspace_idempotently() {
        let base = tmp("ensure");
        let kb = tmp("kbroot");
        let first = ensure(&base, Some(&kb), true).expect("首建应成功");
        assert!(first.config.exists());
        assert!(first.dir.join("AGENTS.md").exists());
        assert!(first
            .dir
            .join(".opencode/skills/kb-ingest/SKILL.md")
            .exists());
        assert!(first
            .dir
            .join(".opencode/skills/kb-research/SKILL.md")
            .exists());

        let raw = std::fs::read_to_string(&first.config).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert!(parsed["permissions"].is_array());

        let mtime = std::fs::metadata(&first.config)
            .and_then(|m| m.modified())
            .unwrap();
        let second = ensure(&base, Some(&kb), true).expect("重建应成功");
        let mtime2 = std::fs::metadata(&second.config)
            .and_then(|m| m.modified())
            .unwrap();
        assert_eq!(mtime, mtime2, "内容未变时不应重写配置");

        let third = ensure(&base, Some(&kb.join("other")), true).unwrap();
        let raw = std::fs::read_to_string(&third.config).unwrap();
        assert!(raw.contains("other/*"), "根目录变化应刷新权限");

        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&kb);
    }

    #[test]
    fn skills_have_frontmatter_and_ids() {
        assert!(SKILL_INGEST.starts_with("---"));
        assert!(SKILL_INGEST.contains("name:"));
        assert!(SKILL_RESEARCH.contains("description:"));
        assert!(AGENTS_MD.contains("硬规则"));
    }
}
