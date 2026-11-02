//! 知识库入库的确定性部分（P13）
//!
//! 职责边界：本模块只做**可验证的文件操作**——校验根目录、给一条资料建独立文件夹、
//! 复制原文件/原目录、写 Markdown。理解内容、起标题、写记忆交给 agent
//! （见 `kb_service` 与 `agent_workspace`）。
//!
//! 目录约定（一条资料一个文件夹）：
//!
//! ```text
//! <root>/<年>/<月>/<主题-slug>-<YYYYMMDD-HHmmss>/
//!   original/            原文件或原目录（原样保留）
//!   <标题>.md            整理后的 Markdown
//! ```
//!
//! 红线：不 shell 拼接、不写 rsrs 数据目录、不覆盖已有文件。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::KnowledgeBaseConfig;

/// 入库来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 单个文件
    File,
    /// 目录（递归）
    Folder,
    /// 网址（抓取后整理）
    Url,
    /// 一段文字（划词 / 手动输入）
    Text,
}

impl Kind {
    pub fn parse(raw: &str) -> Option<Kind> {
        match raw.trim().to_lowercase().as_str() {
            "file" | "文件" => Some(Kind::File),
            "folder" | "dir" | "directory" | "目录" | "文件夹" => Some(Kind::Folder),
            "url" | "link" | "网页" | "链接" => Some(Kind::Url),
            "text" | "文字" | "划词" => Some(Kind::Text),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::File => "file",
            Kind::Folder => "folder",
            Kind::Url => "url",
            Kind::Text => "text",
        }
    }
}

/// 一次入库请求（Tauri 命令与 `/kb/add` 共用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestRequest {
    /// file | folder | url | text；缺省按 value 自动判断
    #[serde(default)]
    pub kind: String,
    /// 文件/目录路径，或网址、正文
    #[serde(default)]
    pub value: String,
    /// 标题（留空由 agent 起）
    #[serde(default)]
    pub title: String,
    /// 正文来源（网页标题、应用名等），仅用于展示与记忆
    #[serde(default)]
    pub source: String,
}

impl IngestRequest {
    /// 解析实际类型：显式 kind 优先，否则看 value 像路径还是网址
    pub fn resolved_kind(&self) -> Result<Kind, String> {
        if let Some(k) = Kind::parse(&self.kind) {
            return Ok(k);
        }
        let v = self.value.trim();
        if v.is_empty() {
            return Err("内容为空".into());
        }
        if v.starts_with("http://") || v.starts_with("https://") {
            return Ok(Kind::Url);
        }
        let p = Path::new(v);
        if p.exists() {
            return Ok(if p.is_dir() { Kind::Folder } else { Kind::File });
        }
        Ok(Kind::Text)
    }
}

/// 限制（来自 `[knowledge_base]`）
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_file_bytes: u64,
    pub max_files: usize,
}

impl Limits {
    pub fn from_config(cfg: &KnowledgeBaseConfig) -> Self {
        Self {
            max_file_bytes: cfg.max_file_bytes(),
            max_files: cfg.max_files.max(1),
        }
    }
}

/// 已准备好的一条资料
#[derive(Debug, Clone, Serialize)]
pub struct Prepared {
    /// 本条资料的独立文件夹
    pub item_dir: String,
    /// 原文件/原目录所在目录
    pub original_dir: String,
    /// 计划写入的 Markdown 路径
    pub md_path: String,
    /// 标题（已去非法字符）
    pub title: String,
    /// 复制后的原文件（相对 original 目录，posix 分隔）
    pub copied: Vec<String>,
    /// 原文件总字节
    pub bytes: u64,
    /// 被跳过的文件及原因
    pub skipped: Vec<String>,
}

/// Windows `canonicalize` 会返回 `\\?\C:\...` 形式；去掉 verbatim 前缀，
/// 让路径能直接展示、拼 prompt，也方便用户复制到资源管理器。
pub fn display_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(rest.to_string());
    }
    path.to_path_buf()
}

/// 校验并创建根目录；返回规范化后的路径
pub fn validate_root(root: &str) -> Result<PathBuf, String> {
    let raw = root.trim();
    if raw.is_empty() {
        return Err("未设置知识库根目录：请在 AI 窗口选择目录。".into());
    }
    let path = PathBuf::from(raw);
    if path.exists() && !path.is_dir() {
        return Err(format!("知识库根目录不是文件夹：{}", path.display()));
    }
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("无法创建知识库根目录 {}：{e}", path.display()))?;
    // 可写性探测：建一个隐藏探针文件后立刻删除
    let probe = path.join(".memtools-write-check");
    std::fs::write(&probe, b"ok")
        .map_err(|e| format!("知识库根目录不可写 {}：{e}", path.display()))?;
    let _ = std::fs::remove_file(&probe);
    std::fs::canonicalize(&path)
        .map(|p| display_path(&p))
        .or(Ok(path))
}

/// 文件名安全化：去掉 Windows 非法字符与控制字符，压缩空白，限制长度
pub fn sanitize_name(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_control() || matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let out = out.trim_matches(['.', ' ', '-']).to_string();
    let mut limited: String = out.chars().take(80).collect();
    // Windows 保留名
    let upper = limited.to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.contains(&upper.as_str()) {
        limited.push('_');
    }
    if limited.is_empty() {
        "未命名".to_string()
    } else {
        limited
    }
}

/// 目录 slug：保留中日韩与字母数字，其余折叠为 `-`，全小写
pub fn slugify(raw: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        let keep = ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch);
        if keep {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let mut s: String = trimmed.chars().take(48).collect();
    s = s.trim_matches('-').to_string();
    if s.is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

/// 在 `<root>/<年>/<月>/` 下建一个唯一文件夹（同日同名追加 `-2`、`-3`）
pub fn create_item_dir(root: &Path, title: &str, stamp: &str) -> Result<PathBuf, String> {
    let (year, month) = split_stamp(stamp);
    let parent = root.join(year).join(month);
    std::fs::create_dir_all(&parent)
        .map_err(|e| format!("无法创建知识库目录 {}：{e}", parent.display()))?;
    let base = format!("{}-{}", slugify(title, "item"), stamp);
    let mut candidate = parent.join(&base);
    let mut n = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{base}-{n}"));
        n += 1;
        if n > 999 {
            return Err("同名资料过多，无法创建目录".into());
        }
    }
    std::fs::create_dir(&candidate)
        .map_err(|e| format!("无法创建资料文件夹 {}：{e}", candidate.display()))?;
    Ok(candidate)
}

/// `20261102-143015` → ("2026", "11")；不合法时用兜底值
fn split_stamp(stamp: &str) -> (String, String) {
    let digits: Vec<char> = stamp.chars().collect();
    if digits.len() >= 6 && digits.iter().all(|c| c.is_ascii_digit() || *c == '-') {
        let year: String = digits[..4].iter().collect();
        let month: String = digits[4..6].iter().collect();
        if !year.is_empty() && !month.is_empty() {
            return (year, month);
        }
    }
    ("0000".into(), "00".into())
}

/// 文本类扩展名（其余格式后续版本再支持）
pub fn is_text_like(path: &Path) -> bool {
    const TEXT_EXT: [&str; 64] = [
        "md", "markdown", "mdx", "txt", "text", "rst", "org", "adoc", "json", "jsonc", "json5",
        "yaml", "yml", "toml", "ini", "cfg", "conf", "env", "csv", "tsv", "log", "xml", "html",
        "htm", "xhtml", "css", "scss", "less", "js", "mjs", "cjs", "ts", "tsx", "jsx", "vue",
        "svelte", "py", "rs", "go", "java", "kt", "kts", "c", "h", "cc", "cpp", "hpp", "cs",
        "php", "rb", "sh", "bash", "zsh", "ps1", "bat", "cmd", "sql", "tex", "r", "lua", "swift",
        "dart", "gradle", "properties",
    ];
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => TEXT_EXT.contains(&ext.to_lowercase().as_str()),
        // 无扩展名：按内容判断（见文本探测）
        None => true,
    }
}

/// 文本文件探测：读前 4 KB，非法 UTF-8 或含 NUL 视为二进制
pub fn looks_like_text(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = vec![0u8; 4096];
    let n = f.read(&mut buf).unwrap_or(0);
    let head = &buf[..n];
    if head.contains(&0u8) {
        return false;
    }
    // 允许最后被截断的多字节字符
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

/// 复制一条资料的原件到 `item_dir/original/`
///
/// - 文件：复制到 `original/<文件名>`
/// - 目录：递归复制到 `original/<目录名>/`
/// - 跳过：超过单文件上限、符号链接、目录本身在知识库内、非文本类二进制（仅记录跳过）
pub fn copy_original(
    source: &Path,
    item_dir: &Path,
    root: &Path,
    limits: Limits,
) -> Result<(PathBuf, Vec<String>, u64, Vec<String>), String> {
    if !source.exists() {
        return Err(format!("源路径不存在：{}", source.display()));
    }
    let source = std::fs::canonicalize(source)
        .map(|p| display_path(&p))
        .unwrap_or_else(|_| source.to_path_buf());
    let canonical_root = std::fs::canonicalize(root)
        .map(|p| display_path(&p))
        .unwrap_or_else(|_| root.to_path_buf());
    if source.starts_with(&canonical_root) {
        return Err("源路径已在知识库内，无需重复入库".into());
    }

    let original_dir = item_dir.join("original");
    std::fs::create_dir_all(&original_dir)
        .map_err(|e| format!("无法创建原件目录 {}：{e}", original_dir.display()))?;

    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".into());
    let target = original_dir.join(sanitize_name(&name));

    let mut copied: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut bytes: u64 = 0;

    if source.is_file() {
        let size = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
        if limits.max_file_bytes > 0 && size > limits.max_file_bytes {
            return Err(format!(
                "文件过大：{}（{} MB，上限 {} MB）",
                name,
                size / 1024 / 1024,
                limits.max_file_bytes / 1024 / 1024
            ));
        }
        if !is_text_like(&source) || !looks_like_text(&source) {
            skipped.push(format!("{name}（非文本格式，本版本只登记路径）"));
        }
        std::fs::copy(&source, &target)
            .map_err(|e| format!("复制失败 {} → {}：{e}", source.display(), target.display()))?;
        copied.push(sanitize_name(&name));
        bytes += size;
        return Ok((original_dir, copied, bytes, skipped));
    }

    if !source.is_dir() {
        return Err(format!("不支持的源类型：{}", source.display()));
    }
    let base = target.clone();
    std::fs::create_dir_all(&base)
        .map_err(|e| format!("无法创建目录 {}：{e}", base.display()))?;
    let mut count = 0usize;
    let mut stack = vec![source.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("无法读取目录 {}：{e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                skipped.push(format!("{}（符号链接）", path.display()));
                continue;
            }
            let rel = path.strip_prefix(&source).unwrap_or(&path);
            let dest = base.join(rel);
            if meta.is_dir() {
                let _ = std::fs::create_dir_all(&dest);
                stack.push(path);
                continue;
            }
            count += 1;
            if count > limits.max_files {
                return Err(format!(
                    "文件数超过上限（{}）：请分批入库",
                    limits.max_files
                ));
            }
            if limits.max_file_bytes > 0 && meta.len() > limits.max_file_bytes {
                skipped.push(format!("{}（超过 {} MB）", rel.display(), limits.max_file_bytes / 1024 / 1024));
                continue;
            }
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::copy(&path, &dest) {
                Ok(_) => {
                    bytes += meta.len();
                    copied.push(rel.to_string_lossy().replace('\\', "/"));
                    if !is_text_like(&path) || !looks_like_text(&path) {
                        skipped.push(format!("{}（非文本格式，本版本只登记路径）", rel.display()));
                    }
                }
                Err(e) => skipped.push(format!("{}（复制失败：{e}）", rel.display())),
            }
        }
    }
    Ok((original_dir, copied, bytes, skipped))
}

/// 写 Markdown；目标已存在时追加 `-2`，绝不覆盖
pub fn write_markdown(item_dir: &Path, title: &str, body: &str) -> Result<PathBuf, String> {
    let name = sanitize_name(if title.trim().is_empty() { "未命名" } else { title.trim() });
    let mut target = item_dir.join(format!("{name}.md"));
    let mut n = 2;
    while target.exists() {
        target = item_dir.join(format!("{name}-{n}.md"));
        n += 1;
        if n > 99 {
            return Err("同名 Markdown 过多，无法落盘".into());
        }
    }
    std::fs::write(&target, body)
        .map_err(|e| format!("写入 Markdown 失败 {}：{e}", target.display()))?;
    Ok(target)
}

/// 拼「来源说明」行，供 Markdown 与记忆复用
pub fn source_note(req: &IngestRequest, kind: Kind) -> String {
    let source = req.source.trim();
    match kind {
        Kind::Url => {
            if source.is_empty() {
                req.value.trim().to_string()
            } else {
                format!("{source} · {}", req.value.trim())
            }
        }
        Kind::Text => source.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "memtools-kb-{}-{}-{}",
            name,
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
    fn kind_parse_aliases() {
        assert_eq!(Kind::parse("file"), Some(Kind::File));
        assert_eq!(Kind::parse("文件夹"), Some(Kind::Folder));
        assert_eq!(Kind::parse(" 网页 "), Some(Kind::Url));
        assert_eq!(Kind::parse("nope"), None);
    }

    #[test]
    fn request_resolves_paths_and_urls() {
        let dir = tmp("resolve");
        let file = dir.join("a.md");
        std::fs::write(&file, "x").unwrap();
        let mut req = IngestRequest {
            kind: String::new(),
            value: file.display().to_string(),
            title: String::new(),
            source: String::new(),
        };
        assert_eq!(req.resolved_kind().unwrap(), Kind::File);
        req.value = dir.display().to_string();
        assert_eq!(req.resolved_kind().unwrap(), Kind::Folder);
        req.value = "https://example.com/x".into();
        assert_eq!(req.resolved_kind().unwrap(), Kind::Url);
        req.value = "一段普通文字".into();
        assert_eq!(req.resolved_kind().unwrap(), Kind::Text);
        req.value = "  ".into();
        assert!(req.resolved_kind().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_removes_windows_illegal_chars() {
        assert_eq!(sanitize_name("a/b:c*d?e\"f<g>h|i"), "a-b-c-d-e-f-g-h-i");
        assert_eq!(sanitize_name("  ..名字..  "), "名字");
        assert_eq!(sanitize_name(""), "未命名");
        assert_eq!(sanitize_name("CON"), "CON_");
        assert!(sanitize_name(&"长".repeat(200)).chars().count() <= 80);
    }

    #[test]
    fn slug_keeps_cjk_and_limits_length() {
        assert_eq!(slugify("Rust 所有权 / 借用", "item"), "rust-所有权-借用");
        assert_eq!(slugify("///", "item"), "item");
        assert!(slugify(&"测".repeat(120), "item").chars().count() <= 48);
    }

    #[test]
    fn display_path_strips_windows_verbatim_prefix() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Users\lenovo\kb")),
            PathBuf::from(r"C:\Users\lenovo\kb")
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\kb")),
            PathBuf::from(r"\\server\share\kb")
        );
        assert_eq!(
            display_path(Path::new(r"C:\plain\path")),
            PathBuf::from(r"C:\plain\path")
        );
    }

    #[test]
    fn item_dir_is_dated_and_unique() {        let root = tmp("itemdir");
        let a = create_item_dir(&root, "Rust 所有权", "20261102-143015").unwrap();
        assert!(a.starts_with(root.join("2026").join("11")));
        assert!(a.file_name().unwrap().to_string_lossy().ends_with("-20261102-143015"));
        let b = create_item_dir(&root, "Rust 所有权", "20261102-143015").unwrap();
        assert_ne!(a, b);
        assert!(b.file_name().unwrap().to_string_lossy().ends_with("-2"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_file_preserves_name_and_reports_skips() {
        let root = tmp("copy-file");
        let item = create_item_dir(&root, "笔记", "20261102-143015").unwrap();
        let src = root.parent().unwrap().join(format!(
            "memtools-src-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&src, "hello").unwrap();
        let (orig, copied, bytes, skipped) =
            copy_original(&src, &item, &root, Limits::from_config(&KnowledgeBaseConfig::default()))
                .unwrap();
        assert!(orig.join(src.file_name().unwrap()).exists());
        assert_eq!(copied.len(), 1);
        assert_eq!(bytes, 5);
        assert!(skipped.is_empty());
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_rejects_source_inside_root() {
        let root = tmp("inside");
        let item = create_item_dir(&root, "x", "20261102-143015").unwrap();
        let inside = root.join("a.txt");
        std::fs::write(&inside, "x").unwrap();
        let err = copy_original(
            &inside,
            &item,
            &root,
            Limits::from_config(&KnowledgeBaseConfig::default()),
        )
        .unwrap_err();
        assert!(err.contains("已在知识库内"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_folder_recurses_and_skips_binary() {
        let root = tmp("copy-folder");
        let item = create_item_dir(&root, "目录", "20261102-143015").unwrap();
        let src = tmp("src-tree");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.md"), "# a").unwrap();
        std::fs::write(src.join("sub/b.txt"), "b").unwrap();
        std::fs::write(src.join("bin.dat"), [0u8, 1, 2, 3]).unwrap();

        let (orig, copied, _bytes, skipped) =
            copy_original(&src, &item, &root, Limits::from_config(&KnowledgeBaseConfig::default()))
                .unwrap();
        assert!(orig.join(src.file_name().unwrap()).join("a.md").exists());
        assert!(orig.join(src.file_name().unwrap()).join("sub/b.txt").exists());
        assert_eq!(copied.len(), 3);
        assert_eq!(skipped.len(), 1, "二进制应只登记不读：{skipped:?}");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn over_limit_file_is_rejected() {
        let root = tmp("limit");
        let item = create_item_dir(&root, "x", "20261102-143015").unwrap();
        let src = root.parent().unwrap().join(format!("memtools-big-{}.txt", std::process::id()));
        std::fs::write(&src, "0123456789").unwrap();
        let limits = Limits {
            max_file_bytes: 5,
            max_files: 10,
        };
        let err = copy_original(&src, &item, &root, limits).unwrap_err();
        assert!(err.contains("文件过大"));
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn markdown_never_overwrites() {
        let dir = tmp("md");
        let a = write_markdown(&dir, "标题", "一").unwrap();
        let b = write_markdown(&dir, "标题", "二").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(a).unwrap(), "一");
        assert_eq!(std::fs::read_to_string(b).unwrap(), "二");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn root_validation_creates_and_checks_writable() {
        let dir = tmp("root");
        let root = dir.join("kb");
        let got = validate_root(root.to_str().unwrap()).unwrap();
        assert!(got.exists());
        assert!(validate_root("  ").is_err());
        let file = dir.join("file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(validate_root(file.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
