# AI 来源（agent）安装与配置

AI 窗口左上角的「AI 智能体」下拉列出本机可用的 CLI。每一项都带三条信息：**装没装**（实时探测）、**怎么装**、**怎么登录**。没装或没登录时，点选该项会在下拉下方展开「安装与配置」面板，面板里的命令可直接点「复制命令」粘贴到终端。

- opencode、Qoder CLI：memTools 原生支持，绑定即用。
- Claude Code、Codex CLI：通过 `kind = "custom"` 的参数模板驱动，绑定后 memTools 会自动写入参数（`-p {prompt} --output-format json` 等），无需手写配置。
- 自定义 CLI：任何能把 prompt 当参数吃进去、把回答打到 stdout 的程序。

装完不必重启 memTools：点「重新检测」→ 选中该项 → 点「绑定」。

---

## 1. opencode

```powershell
npm i -g opencode-ai
opencode --version
opencode auth login          # 选 provider 并填 key（也可用环境变量）
opencode models              # 会列出 provider/model，AI 窗口的模型下拉就取这里
```

- 模型填 `provider/model`，例如 `opencode/big-pickle`。
- memTools 走 `opencode run --format json`，会解析事件流。
- 文档：<https://opencode.ai/docs>

## 2. Qoder CLI（qodercli）

Qoder CLI 是 Qoder IDE 自带的可执行文件，**不通过 npm 安装，也不在 PATH 上**，所以直接在 PowerShell 里敲 `qodercli` 会报「无法将 qodercli 项识别为 cmdlet…」。

本机实际位置（可用 memTools 的提示确认）：

```
D:\001program\097qoder\Qoder\resources\app\resources\bin\x86_64_windows\qodercli.exe
```

登录（注意开头那个 `&`，路径含空格时必须加）：

```powershell
& "D:\001program\097qoder\Qoder\resources\app\resources\bin\x86_64_windows\qodercli.exe"
# 进入交互界面后输入 /login
```

登录后可验证：

```powershell
& "D:\001program\097qoder\Qoder\resources\app\resources\bin\x86_64_windows\qodercli.exe" status
# Account: <你的账号>     ← 未登录会显示 Not logged in
```

不想每次写全路径，把该目录加进用户 PATH：

```powershell
$dir = "D:\001program\097qoder\Qoder\resources\app\resources\bin\x86_64_windows"
[Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path","User") + ";$dir", "User")
# 重开终端后生效
qodercli --version
```

- 模型只有固定十档：`auto / efficient / gmodel / kmodel / lite / mmodel / performance / q35model / qmodel / ultimate`。
- 也可以用环境变量免登录：`QODER_PERSONAL_ACCESS_TOKEN`，或写进 `config.toml` 的 `[processor.agent] token`。
- 文档：<https://docs.qoder.com>

## 3. Claude Code（走 custom 预设）

```powershell
npm install -g @anthropic-ai/claude-code
claude --version
claude          # 首次运行，输入 /login 走浏览器授权；或设 ANTHROPIC_API_KEY
```

memTools 绑定时写入的参数：

```
claude -p {prompt} --output-format json
```

输出按 `json` 解析（取 `result` 字段）。模型可留空用 CLI 默认，或填 `claude-sonnet` / `claude-opus` 等。

- 文档：<https://docs.anthropic.com/en/docs/claude-code>

## 4. Codex CLI（走 custom 预设）

```powershell
npm install -g @openai/codex
codex --version
codex login     # 或设 OPENAI_API_KEY
```

memTools 绑定时写入的参数：

```
codex exec {prompt}
```

输出按 `text` 解析。模型可留空，或填 `gpt-5-codex` 等。

- 文档：<https://github.com/openai/codex>

## 5. 自定义 CLI

绑定后把参数写进 `src-tauri/config.toml`（或直接改 `%APPDATA%\com.memtools.desktop\knowledge_base.json`）：

```toml
[processor.agent]
kind = "custom"
bin = "crush"                                   # 命令名或完整路径
args = ["run", "--model", "{model}", "{prompt}"] # 每个元素是一个 argv
env = ["CI=1"]                                   # KEY=VALUE
models = ["claude-sonnet", "gpt-5"]              # 模型下拉候选，可留空
output = "json"                                  # text | json | opencode-json | qoder-json
```

占位符：`{prompt}` `{model}` `{session}` `{endpoint}` `{workdir}` `{agent}`；替换后为空的参数会被丢弃。

两类功能的输出约定：

| output | 期望 stdout | 说明 |
| --- | --- | --- |
| `text` | 纯文本 | 整段 stdout 当回答 |
| `json` | 单个 JSON 对象 | 依次取 `result` / `text` / `content` / `response` / `output` / `message` |
| `opencode-json` | NDJSON 事件流 | 取最后一条 message 的 text 片段 |
| `qoder-json` | `{"type":"result","result":"…"}` | 按 qodercli 的 result 字段取 |

## 排查

| 现象 | 处理 |
| --- | --- |
| 下拉显示「未就绪」 | 点该项看下方「安装与配置」面板，按顺序装 → 登录 → 重新检测 |
| 提示「无法将 xxx 项识别为 cmdlet」 | 该 CLI 不在 PATH：用完整路径，或把目录加进用户 PATH 后重开终端 |
| 提示未登录 | 按面板的登录命令执行；Qoder 是启动 CLI 后输入 `/login` |
| 绑定成功但一跑就报错 | 点「重新检测」看版本；再确认模型名对不对（opencode 要 `provider/model`） |
| 改完 config.toml 不生效 | 重新点「绑定」，让界面选择覆盖到 `knowledge_base.json`；界面选择优先于 config.toml |
