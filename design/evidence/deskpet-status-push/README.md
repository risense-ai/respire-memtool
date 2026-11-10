# DeskPet 状态推送 — 界面证据

设置面板新增「DeskPet 状态推送」开关（通过本机 DeskPet MCP 把 memTools 状态发到桌宠气泡）。

| 图片 | 版本 | 状态 |
| --- | --- | --- |
| `before.png` | `main` @ `30e3a70` | 设置面板没有 DeskPet 行 |
| `after-off.png` | 本 PR | 新增 DeskPet 行，默认关闭 |
| `after-on.png` | 本 PR | 开关打开后的状态行 |

三张图使用同一场景、同一视口（480×560 逻辑像素，2× 设备像素比）和浅色主题，由
`devtools/ui-preview.mjs` 的 UI fixture 实际渲染后截图。截图中的 Tauri 命令结果来自
fixture 测试桩；`after-on.png` 的「已连接」是测试桩返回的界面状态，真实 MCP
握手 / `pet_say` / 会话失效重连由 Rust 单测覆盖：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib deskpet
```
