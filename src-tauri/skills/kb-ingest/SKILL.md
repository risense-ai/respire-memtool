---
name: 知识库入库
description: 把 memTools 选中的文件或文件夹整理成一篇 Markdown 存入知识库，并按 rsrs 规范写入记忆。由 memTools 的「添加到知识库」自动调用。
---

# 知识库入库（kb-ingest）

任务正文会给出：

- `ITEM_DIR`：本条资料的独立文件夹（`original/` 下是原文件或原目录）
- `MD_PATH`：必须写入的 Markdown 绝对路径
- `SOURCE_NOTE`：来源说明（可能为空）

## 步骤

1. 列出 `ITEM_DIR/original/` 下的内容。
2. 逐个读取文本类文件（`.md/.txt/.csv/.json/.yaml/.toml/代码/日志…`）。
   非文本格式（pdf/docx/xlsx/图片/压缩包）**不要尝试解析**，只在文档里登记文件名与大小。
   单目录超过 40 个文本文件时，只精读前 20 个最主要的，其余按文件名列入清单。
3. 写 Markdown 到 `MD_PATH`（UTF-8，绝不写到别的路径）。结构：

```markdown
# <标题：12-24 字，写清这份资料是什么>

> 整理日期：YYYY-MM-DD ｜ 来源：<SOURCE_NOTE 或 本地文件>

## 一句话

<这份资料解决什么问题 / 讲了什么，1-2 句>

## 要点

- <可复用的结论，带数字与专有名词>
- ...

## 原文摘录

<关键段落或代码片段，最多 30 行；没有则省略本节>

## 文件清单

- `original/xxx.md`（12 KB）
- ...

## 备注

<不确定、缺失、后续要核实的点；没有则写「无」>
```

4. 写记忆（严格遵守 rsrs 规范，宁慢勿错）：

```powershell
rsrs recall "知识库 <主题关键词>" --titles --json --limit 10
```

- 命中同题条目 → 走 **改**：`rsrs update <id> --content "…" --title "…"`；或 **并**：`rsrs remember "<综合版>" --merge-ids "<id1,id2>"`
- 同题但属于同一主题面 → **挂**：`rsrs remember "<正文>" --title "<标题>" --parent <候选id> --importance important`
- 确无同题 → **存**：`rsrs remember "<正文>" --title "<标题>" --parent <知识库纲节点id> --importance important --tags "知识库,<主题>,设备绑定"`

记忆正文必须三段（一段一句，直陈）：

```text
【前因】<为什么收这份资料：谁/什么场景需要它>
【行为】<整理成哪篇文档、放在哪个目录、读了哪些原件>
【后果】<这份资料能解决什么问题、有什么缺口或后续动作>
```

`【行为】` 必须含 `MD_PATH`、`ITEM_DIR` 与设备（主机名/系统，如 `DESKTOP-XXX/win`）。
新建前必须先 recall 判重；找不到挂点就 `rsrs tree --outline --json` 再找，不要留孤儿条。
写完 `rsrs show <id> --json` 回读确认正文含路径。

5. 最后**只输出一行**结果，供 memTools 解析：

```text
KB_RESULT: {"title":"<标题>","memory_id":"<记忆 id，失败留空>","summary":"<30 字摘要>","note":"<可选：跳过或失败的说明>"}
```

- `memory_id` 必须从 `rsrs show <id> --json` 的输出里**原样复制完整 UUID**，不要凭记忆手打。
- `summary` 只写这份资料的内容（讲了什么、能解决什么），不要写工具执行情况、命令报错或其它过程细节。
