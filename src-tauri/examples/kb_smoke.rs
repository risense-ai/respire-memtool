//! 知识库入库冒烟测试（真实调用本机 opencode，会写一条 rsrs 记忆）
//!
//! 运行：`cargo run --example kb_smoke -- <源文件或目录> [模型]`
//!
//! 全部产物落在系统临时目录的 `memtools-kb-smoke/` 下，不碰用户真实的
//! `~/AppData/Roaming/com.memtools.desktop` 配置，也不注册右键菜单。

use std::sync::Arc;
use std::time::{Duration, Instant};

use memtools_lib::config::{AgentConfig, KnowledgeBaseConfig, ProcessorConfig};
use memtools_lib::kb::IngestRequest;
use memtools_lib::kb_service::KbService;
use memtools_lib::kb_settings::KbSettings;
use memtools_lib::processor_settings::{ProcessorChoice, ProcessorSettings};

fn main() {
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .expect("用法：cargo run --example kb_smoke -- <源文件或目录> [模型]");
    let model = args
        .next()
        .unwrap_or_else(|| "opencode-go/deepseek-v4.1-flash".to_string());

    let base = std::env::temp_dir().join("memtools-kb-smoke");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("创建临时目录失败");
    let kb_root = base.join("kb");
    let cfg_dir = base.join("cfg");
    std::fs::create_dir_all(&kb_root).expect("创建知识库目录失败");

    let kb_settings = Arc::new(KbSettings::load(cfg_dir.join("knowledge_base.json")));
    kb_settings
        .patch(|c| c.root = kb_root.to_string_lossy().to_string())
        .expect("写入知识库设置失败");

    let processor = Arc::new(ProcessorSettings::load(cfg_dir.join("processor.json")));
    processor
        .set(ProcessorChoice {
            source: "opencode".into(),
            agent_model: model.clone(),
            ..Default::default()
        })
        .expect("写入梳理设置失败");

    let mut kb_cfg = KnowledgeBaseConfig::default();
    kb_cfg.root = kb_root.to_string_lossy().to_string();
    kb_cfg.context_menu = false;
    kb_cfg.timeout_secs = 600;

    let mut proc_base = ProcessorConfig::default();
    proc_base.agent.kind = "opencode".into();
    proc_base.agent.model = model.clone();
    proc_base.agent.timeout_secs = 600;

    let mut agent_base = AgentConfig::default();
    agent_base.kind = "opencode".into();
    agent_base.model = model.clone();
    agent_base.timeout_secs = 600;

    let svc = Arc::new(KbService::new(
        Arc::new(|event: &str, payload: serde_json::Value| {
            let text = payload.to_string();
            let brief: String = text.chars().take(300).collect();
            println!("EVENT {event} {brief}");
        }),
        kb_cfg,
        kb_settings,
        processor,
        proc_base,
        agent_base,
        cfg_dir,
    ));

    let task = svc
        .submit(IngestRequest {
            kind: String::new(),
            value: source.clone(),
            title: String::new(),
            source: String::new(),
        })
        .expect("提交失败");
    println!("TASK {} kind={}", task.id, task.kind);

    let started = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(3));
        let list = svc.tasks();
        let Some(now) = list.iter().find(|t| t.id == task.id) else {
            continue;
        };
        if now.status == "done" || now.status == "failed" {
            println!("RESULT status={}", now.status);
            println!("title={}", now.title);
            println!("item_dir={}", now.item_dir);
            println!("md_path={}", now.md_path);
            println!("memory_id={}", now.memory_id);
            println!("files={} bytes={}", now.files, now.bytes);
            println!("message={}", now.message);
            break;
        }
        if started.elapsed() > Duration::from_secs(900) {
            println!("TIMEOUT 最后阶段：{}", now.stage);
            break;
        }
    }

    println!("KB_ROOT={}", kb_root.display());
}
