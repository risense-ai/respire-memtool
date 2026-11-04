# memTools 功能零缺失验收基线（functional parity）

> 用途：UI 重构后逐条勾选对照，**功能一个都不能少**。
> 来源（只读）：全文通读 `index.html`(389)、`src/main.js`(3015)、`src/ai.html`+`src/ai.js`(784)、`selection.html`+`src/selection.js`(450)、`src/styles.css`(1952)、`src/graph/graphView.js`/`interaction.js`、`src/memory-browser.js`、`src/diary-fragments.js`；`src-tauri/src/lib.rs` 按需通读命令注册表（`generate_handler!` 61 条）、`setup` 初始化、托盘/热键/窗口、`submit_memory`、`automation_submit/resolve`、更新与 rsrs/自启命令、持久化落点；另核对 `tauri.conf.json`、`capabilities/default.json`、`config.toml`、`selection.rs` 尺寸逻辑、`server.rs` 端点、`recent.rs`/`window_position.rs`。
> 版本锚点：本基线与 Git HEAD `9da574d` 的**原始文件**对齐（index.html 389 行 / main.js 3015 行 / styles.css 1952 行）。
> 行号均指 HEAD 版本；工作树正在被改造改动，行号会漂移，核对请用 `git show HEAD:src/main.js | sed -n 'Np'` 之类的只读命令，不要按工作树当前行号对。
> `src/theme/`、`design/` 为未跟踪新增目录（当前未被任何 HTML/JS 引用，见 M 节最后两条）。

## 0. 标注口径与使用说明

- 全部条目均为**静态阅读**结论，本次未运行应用窗口。标注分两类：
  - （代码确认）＝ 绑定、控件 id、命令名、分支条件可在源码逐行核对；不代表运行时已实测。
  - （静态推断，未运行验证）＝ 行为依赖 Rust 侧实现、CSS 断点、系统窗口/托盘交互或视觉效果，只读代码无法证明实际表现。
- 格式：`- [ ] 视图/区域 · 控件名(id) — 功能一句话 — 依赖命令/函数 — 触发方式`
- 勾选规则：改造后每条都能在新 UI 找到对应入口，且状态/空态/错误态一致，方可勾选。
- 计数已按「## 分节」统计，见文末 P 节；分节标题不得随意改动，否则计数失效。

## A. 外壳 · 顶栏 · 视图切换（7 条）

- [ ] 外壳 · 品牌区 `.brand`（含 `.logo-dot`/`.brand-name`/`.brand-sub`）— 静态标识「memTools / rsrs 记忆」，窄窗（≤430px）隐藏副标题 — 无 — 自动（静态推断，未运行验证）
- [ ] 外壳 · AI 按钮 `#btn-ai` — 打开独立 AI 与知识库窗口（不切换主窗视图） — `ai_open` — 点击（代码确认）
- [ ] 外壳 · 最小化按钮 `#btn-min` — 隐藏到托盘，进程常驻（不是最小化到任务栏，主窗 `skipTaskbar: true`） — `hide_window` — 点击（代码确认）
- [ ] 外壳 · 关闭按钮 `#btn-close` — 同样只隐藏到托盘，真正退出仅走托盘「退出 memTools」或 `quit_app` — `hide_window` — 点击（代码确认）
- [ ] 外壳 · 标题栏拖拽区 `#titlebar` — 整条可拖动窗口；按钮/输入/select/textarea/a/`[data-no-drag]` 除外；`startDragging` 不可用时退回 `data-tauri-drag-region` — `window.getCurrentWindow().startDragging()`；capability `core:window:allow-start-dragging` — 鼠标左键按下（代码确认）
- [ ] 外壳 · 缩放手柄 `#win-grips` 8 向（`.win-grip[data-resize=North|South|West|East|NorthWest|NorthEast|SouthWest|SouthEast]`）— 无边框窗口四边四角拖拽缩放；无 `startResizeDragging` 时整个 `#win-grips` 被移除 — `startResizeDragging(direction)`；capability `core:window:allow-start-resize-dragging` — 鼠标左键 `mousedown` / 触点 `touchstart`（代码确认）
- [ ] 视图切换 · 六个 tab `#view-tabs .view-tab[data-view=write|browse|search|remind|targets|graph]` — 切换主区视图并置 active；切走时清结果区；`browse` 懒加载、`search` 聚焦搜索框、`remind` 拉提醒、`targets` 拉目标、`graph` 进图谱、`write` 聚焦输入 — `switchView()` — 点击（代码确认）

## B. 记录视图 `#view-write`（24 条）

- [ ] 记录 · 输入模式切换 `#input-modes .mode-btn[data-mode=free|sections]` — 自由文本 / 三段模板互斥；切 free 隐藏 `#sections-wrap` 并聚焦 `#memory-input`，切 sections 隐藏 `.input-wrap` 并聚焦 `#sec-action` — `setInputMode()` — 点击（代码确认）
- [ ] 记录 · 自由文本输入 `#memory-input` — 主输入框（占位文案含【前因】【行为】【后果】提示），参与字数统计与提交正文 — `getContent()` — 输入（代码确认）
- [ ] 记录 · 三段模板 `#sections-wrap` + `#sec-cause`（前因，可空）+ `#sec-action`（行为，必填）+ `#sec-effect`（后果，可空）— 拼成 `【前因】…\n【行为】…\n【后果】…` 作为提交正文，空段不占行 — `sectionContent()` — 输入（代码确认）
- [ ] 记录 · 字数统计 `#char-count` — 按「当前待提交正文」实时计数（三段模式统计拼接后正文）；任一输入（含 `#trigger-input`）触发 — `updateCharCount()` — 自动/输入（代码确认）
- [ ] 记录 · 梳理按钮 `#process-btn` — 切换 `#process-bar` 显隐并置 `.active`；展开时重新拉 `processor_status`，关闭态给出「梳理已关闭」提示；`title` 随来源/开关变化 — `processor_status`、`updateProcessBtnTitle()` — 点击（代码确认）
- [ ] 记录 · 梳理来源条 `#process-bar` + `#proc-source` + `#proc-source-opts`（动态按钮）— 展示 模型 / opencode / Qoder / 关闭（以 `processor_status.options` 为准），点选即切换并落盘（processor.json），无需改 config.toml 或重启；切换中容器加 `.busy` — `processor_set_source`、`processor_probe`、`renderProcessorSource()` — 点击（代码确认）
- [ ] 记录 · 来源提示 `#proc-source-hint` — 切换结果提示：模型显示 endpoint/model 或「未配置模型端点」；agent 显示探测中/探测结果；`.ok`/`.err` 配色 — `setProcHint()`、`processor_probe` — 自动（代码确认）
- [ ] 记录 · 六个梳理任务 `#process-bar .process-task[data-task=summarize|structure|title|translate|extract|polish]` — 调 LLM/agent 处理当前正文；成功后回填输入框、切回自由文本、结果区显示「已梳理 / 已起标题」并聚焦输入 — `process_memory`、`runProcess()` — 点击（代码确认）
- [ ] 记录 · 梳理校验分支 — 空内容→warn「内容为空」；`processor_enabled=false`→warn「梳理已关闭」；正文 >8000 字→err「内容过长」；命令无返回/`res.ok=false`→err「梳理失败」 — 前端守卫 + `process_memory` — 点击（代码确认）
- [ ] 记录 · 触发行 `#trigger-row` + `#trigger-input` — 仅当「手动 / 主区 / 类型=task / 正文不含【触发】」时显示；`submit_memory` 返回 `needs_trigger` 时强制显示并聚焦 — `updateTriggerRow()`、`submit_memory` — 自动/点击（代码确认）
- [ ] 记录 · 自动化提示词 `#hint-wrap` + `#hint-input` — 仅自动化开启时显示，作为 `automation_submit` 的 `hint`（项目名/关键词，提升判重准确度） — `automation_submit` — 输入（代码确认）
- [ ] 记录 · 去向回执行 `#write-destination`（`role=status`）— 三态文案：自动化=「去向：自动判断…」；日记=「去向：当日活动轨迹（日记）…」；主区=「去向：主记忆 / <已选上级标题|树名|请选择上级>」 — `updateDestination()` — 自动（代码确认）
- [ ] 记录 · 层一去向 `#level-1 .seg-btn[data-target=main|journal]` — 主区（important）/ 日记（trivial）；切日记隐藏层二/三并清结果区；切主区在未选树时自动选中第一棵树 — `setTarget()` — 点击（代码确认）
- [ ] 记录 · 层二分类树 `#level-2` + `#tree-groups`（按 domain 分「人域 · Human」「AI 域 · AI」两组 chips，`data-name`/`data-id`）— 选树根；选中会清空已选上级（`selectedParent`） — `renderTree()`、`selectTree()`、`get_taxonomy` — 点击（代码确认）
- [ ] 记录 · 层二上级节点搜索 `#parent-query`（`type=search`，180ms 防抖）+ `#parent-results` — 在已加载全量记忆里筛非日记、未取代的主记忆（最多 12 条），点选后记为真实上级 ID、清空树 chips 高亮与输入；无命中显示「没有可选的有效主记忆节点。」；读取失败显示「读取上级失败：…」 — `searchParents()`、`ensureMemoryCatalog()` → `list_recent` — 输入/点击（代码确认）
- [ ] 记录 · 层三性质标记 `#level-3` + `#type-pills`（7 项：context/decision/preference/task/emotion/time/skill）— 单选，默认 context；改选会重算触发行 — `renderTypes()`、`selectedType` — 点击（代码确认）
- [ ] 记录 · 结果区 `#result-area`（`ok|info|err|warn` 四态）— 承载提交回执（action/id/挂点/标题/分支/判档/路径/目标树）、梳理进度、校验提示 — `setResult()`/`clearResult()` — 自动（代码确认）
- [ ] 记录 · 落库回执 `showDone` — 显示「已落库 ✓」+ `action/id/挂点/标题/分支(importance)/判档(模型|本地)/message`；`memory_path` 补祖先路径；`branch=attach` 时 `memory_outline(depth:1)` 显示目标树前 8 节点；成功时 `#runtime-dot` 绿闪 2 秒 — `memory_path`、`memory_outline`、`flashOk()` — 自动（代码确认）
- [ ] 记录 · 回执「查看记录 · xxxx」按钮 — 跳到搜索视图、按 id 快速搜索并展开对应卡片 — `viewSavedMemory()` → `switchView`/`runSearch` — 点击（代码确认）
- [ ] 记录 · 判重候选择一 `#result-area .choice-list`（复选框，首项默认勾选，显示标题与相似度）+ `.choice-actions` — 「改 / 并 / 挂」需勾选候选（未勾选→warn「未选择候选」）；「并」传多候选逗号串；「存新条」配 `#choice-tree` 分类下拉（默认当前树） — `automation_resolve`、`renderChoices()`、`doResolve()` — 点击（代码确认）
- [ ] 记录 · 提交主流程与校验分支 — 三段模式下「行为」必填；正文空→warn；>8000 字→err；手动主区且未选树且未选上级→warn「未选择分类」；手动走 `submit_memory(target, content, tree, type, trigger)`，自动化走 `automation_submit(content, hint)` — `submit()` — `Ctrl+Enter` / 快捷键（代码确认）
- [ ] 记录 · 提交返回分支 — `ok`→清空输入并展示回执；`needs_trigger`→显触发行并聚焦；`needs_confirm`+候选→候选择一 UI；其余→err「提交失败」；抛异常→err — `submit_memory`/`automation_submit`、`handleAutomation()` — 自动（代码确认）
- [ ] 记录 · 落库后清空 `resetInputs()` — 清空自由文本、三段、触发行、提示词，重置字数，标记 `browseDirty` 并 `catalogRevision++`（下次进浏览/搜索重取全量） — 无量 — 自动（代码确认）
- [ ] 记录 · 自动化开关下的层级收折 — 开启自动化后隐藏层一，层二/层三一并隐藏（去向由引擎判定），显示提示词行与「模型判档·判树 / 本地判档·判树」状态 — `setAuto()` — 自动（代码确认）

## C. 最近记忆 `#view-browse`（12 条）

- [ ] 最近记忆 · 快速筛选 `#browse-query`（`type=search`，120ms 防抖）— 标题/正文/id 多词 AND 过滤，命中即清空折叠状态并重绘（不发新请求） — `renderRecent()`、`selectMemories()` — 输入（代码确认）
- [ ] 最近记忆 · 范围下拉 `#browse-count`（全部/最近20/50/100/200）— `change` 触发重绘；`Enter` 也刷新；「全部」= limit 0（不截断，全量快照） — `loadRecent()`、`list_recent({limit:0})` — 选择/回车（代码确认）
- [ ] 最近记忆 · 展示方式 `#browse-view-mode[data-mode=tree|list]` — 树形（补祖先路径）/ 列表；切换只重绘不重取；同时维护 `aria-pressed` — `renderMemoryCollection()`、`memoryForest()` — 点击（代码确认）
- [ ] 最近记忆 · 刷新 `#browse-refresh` — 强制重取全量（`force=true` 递增 catalogRevision，丢弃旧响应） — `loadRecent(true)` → `list_recent` — 点击（代码确认）
- [ ] 最近记忆 · 资料类型 `#browse-source`（全部资料/主记忆/日记）— 客户端二次过滤（日记判定：importance=trivial 或标题「活动轨迹 YYYY-MM-DD」） — `renderRecent()`、`isDiary()` — 选择（代码确认）
- [ ] 最近记忆 · 分类 chips `#browse-cats`（「全部」+ 分类树名）— 按真实祖先链匹配（分类名/ID 命中即含其后代）；每个 chip 显示命中数，空分类加 `.empty-category` 并 `title=当前分类暂无记忆`；选中维持 `aria-pressed`；切换清折叠并重绘 — `renderBrowseCats()`、`selectMemories()` — 点击（代码确认）
- [ ] 最近记忆 · 状态行 `#browse-status`（`role=status`）— 「正在读取全部有效记忆…」/「筛选中…」/「显示 N 条 / 当前范围 M 条 · 全库 K 条」；失败为 `.err`「读取失败：…」 — `loadRecent()` — 自动（代码确认）
- [ ] 最近记忆 · 列表容器 `#browse-list` — 空态文案：分类内无匹配「当前分类暂无匹配记忆。」，否则「没有匹配的记忆。」 — `renderMemoryCollection()` — 自动（代码确认）
- [ ] 最近记忆 · 记忆卡片 `.mem-item`（`data-memory-id`，`role=button`，`tabIndex=0`）— 标题 + 元信息（更新时间或相似度、kind、证据标签、importance）；点击或 `Enter`/`Space` 展开正文；`.expanded .mem-content` 才显示 — `buildMemItem()` — 点击/键盘（代码确认）
- [ ] 最近记忆 · 正文片段 `.mem-fragment`（最多 3 段，每段 360 UTF-16 单元，带 `[hh:mm]` 分节时间与 id 前 8 位）— 点击展开并滚动定位高亮命中；`queryPattern` 转义后 `<mark>` 高亮 — `matchingFragments()`、`highlight()` — 点击（代码确认）
- [ ] 最近记忆 · 取代提示 `.mem-super` — 有 `superseded_by` 时显示「已被 xxx 取代 · 点击查看新条」，点击跳搜索该 id — `switchView('search')`/`runSearch()` — 点击（代码确认）
- [ ] 最近记忆 · 树形折叠 `.tree-fold`（`▾/▸`，`aria-expanded`/`aria-label`）— 有子节点可折叠（默认根层展开，搜索时全展开）；无子节点为禁用「·」，`aria-hidden`；补足的祖先卡片加 `.path-context`（虚线、隐藏元信息、title「上级路径，不计入匹配数量」） — `renderMemoryCollection()`、`memoryForest()` — 点击（代码确认）

## D. 搜索 `#view-search`（8 条）

- [ ] 搜索 · 关键词框 `#search-input`（`type=search`）— 关键词模式下 120ms 防抖即时搜索；语义模式下不自动触发（回车/按钮）；占位文案随模式切换 — `runSearch()`、`ensureMemoryCatalog()` — 输入（代码确认）
- [ ] 搜索 · 搜索按钮 `#search-btn`（primary）— 立即执行当前模式搜索 — `runSearch()` — 点击（代码确认）
- [ ] 搜索 · 搜索方式 `#search-kind[data-kind=quick|semantic]` — 关键词（全量快照本地筛）/ 语义（`search_memory`）；切换时清计时器、`searchReq++`、更新占位文案、显示或隐藏数量框并立即执行 — `runSearch()`、`search_memory` — 点击（代码确认）
- [ ] 搜索 · 结果展示方式 `#search-view-mode[data-mode=tree|list]` — 树形补祖先 / 列表；切换只重绘已有结果（`searchResult`） — `renderSearch()` — 点击（代码确认）
- [ ] 搜索 · 数量 `#search-count-wrap` + `#search-count`（number 1–100，默认 20）— 仅语义模式显示；实际取值 clamp 到 1..100；`Enter` 触发搜索 — `runSearch()` — 输入/回车（代码确认）
- [ ] 搜索 · 状态行 `#search-status`（`role=status`）— 默认「输入关键词，搜索全部有效记忆。」；「筛选中…」/「语义搜索中…」；结果为「全库关键词匹配 N 条」/「语义匹配 N 条」/「精确匹配 N 条」，树形模式追加「保留上级路径 / 正在补全上级路径 / 上级路径未能读取」；失败 `.err`「搜索失败：…」；`title` 承载响应 message — `renderSearch()` — 自动（代码确认）
- [ ] 搜索 · 结果容器 `#search-list` — 复用记忆卡片；空态「没有匹配的记忆。」；语义/id 搜索失败补全路径时保留命中项并标记 `pathIncomplete` — `renderMemoryCollection()` — 自动（代码确认）
- [ ] 搜索 · 竞态与空查询 — 空查询清空列表并复位状态；`searchReq` 序号丢弃过期响应；语义结果在快照未就绪时异步补全祖先 — `runSearch()` — 自动（代码确认）

## E. 提醒 `#view-remind`（8 条）

- [ ] 提醒 · 状态行 `#remind-status` — 「扫描中…」；成功后「核对日期 <checked_at> · 逾期 N · 今天 N · 即将 N」，`truncated` 时追加「仅扫描最近 M 条，更早任务可能未纳入」；失败 `.err`「提醒扫描失败：…」 — `loadReminders()`、`list_reminders` — 自动（代码确认）
- [ ] 提醒 · 刷新 `#remind-refresh` — 重新扫描 — `loadReminders()` — 点击（代码确认）
- [ ] 提醒 · 分组容器 `#remind-groups` — 四组固定顺序 已逾期/今天到期/即将到期/之后，空组不渲染；整表为空时显示「没有带【触发】日期的 task。存 task 时填好日期就会出现在这里。」 — `renderReminders()`、`REMIND_GROUPS` — 自动（代码确认）
- [ ] 提醒 · 分组标题 `.remind-group-head` — 「<组名> · N」，`.overdue/.today/.soon` 决定标题与卡片左边框配色 — 无量 — 自动（静态推断，未运行验证）
- [ ] 提醒 · 条目 `.remind-item`（无 id，动态生成，含 `.remind-due`/`.remind-title`/`.remind-days`/`.remind-trigger`/`.mem-content`/`.remind-actions`）— 截止日期、标题、剩余天数（「逾期 N 天/今天/N 天后」）、【触发】原文、正文；点击卡片切换 `.expanded` 显示正文与常亮操作 — `buildReminderItem()` — 点击（代码确认）
- [ ] 提醒 · 完成 `reminder_complete` — 按钮置灰显示「完成中」；成功后重扫；失败显示「完成失败」并把错误放 `title`，1.8 秒后复位 — `reminder_complete({id})` — 点击（代码确认）
- [ ] 提醒 · 忽略 `reminder_dismiss` — 按钮置灰；成功后重扫；失败显示「忽略失败」+ `title` 错误（触发日期变更后会重新出现） — `reminder_dismiss({id, due})` — 点击（代码确认）
- [ ] 提醒 · 查看 `查看` 按钮 — 跳搜索视图按 id 搜索 — `switchView('search')`/`runSearch()` — 点击（代码确认）

## F. 快捷目标 `#view-targets`（14 条）

- [ ] 快捷目标 · 说明文案 `.targets-hint` — 静态「第 1 个 = 最优先。划词悬浮窗与网页插件共用这份列表。」 — 无 — 自动（静态推断，未运行验证）
- [ ] 快捷目标 · 状态行 `#targets-status` — 「读取中…」/「N 个目标」/「有未保存的修改」/「保存中…」/「已保存 N 个目标」/「恢复中…」/「已恢复默认目标」；错误 `.err` 前缀「读取失败： / 保存失败： / 恢复失败：」 — `setTargetsStatus()` — 自动（代码确认）
- [ ] 快捷目标 · 列表 `#targets-list` — 空态「还没有快捷目标，点「新增目标」加一个。」；每行 `.target-row[data-target-id]` — `renderTargets()`、`quick_targets_list` — 自动（代码确认）
- [ ] 快捷目标 · 行内序位徽标 `.target-badge` — 第 1 行显示「优先」+ `.primary`，其余显示序号；上移/下移/删除后立即重排 — `refreshTargetBadges()` — 自动（代码确认）
- [ ] 快捷目标 · 名称输入 `.target-name`（`maxLength=24`，占位「目标名称」）— 编辑即标记未保存 — `markTargetsDirty()` — 输入（代码确认）
- [ ] 快捷目标 · 上移/下移/删除 `.target-icon`（↑「上移（更优先）」/↓「下移」/✕「删除该目标」）— 首尾越界不动；删除后重排序位并标记未保存 — `moveTargetRow()`/`removeTargetRow()` — 点击（代码确认）
- [ ] 快捷目标 · 类型下拉 `.target-kind`（日记 / 挂到节点 / 自动判档）— 非「挂到节点」时隐藏父节点、type、importance 三个字段（`syncTargetKind`），保存时置 null — `syncTargetKind()`、`collectTargets()` — 选择（代码确认）
- [ ] 快捷目标 · 父节点 `.target-parent`（占位「分类树名或记忆 id」，绑定 `list=target-tree-list`）+ 类型 `.target-type`（7 类）+ 重要度 `.target-importance`（重要/一般）— 仅 tree 模式有效；父节点候选取自 `quick_targets_*` 返回的 `taxonomy`，缺失时退回 `get_taxonomy` 结果 — `fillTargetTreeList()` — 输入/选择（代码确认）
- [ ] 快捷目标 · 候选数据源 `#target-tree-list`（datalist）— 由 taxonomy 名称填充，供 `.target-parent` 自动补全 — `fillTargetTreeList()` — 自动（代码确认）
- [ ] 快捷目标 · 新增 `#targets-add` — 上限 8 个（超出→err「最多 8 个快捷目标」）；新行 id `t<时间戳>` 且唯一；默认 kind=tree/type=context/importance=important；插入后聚焦并全选名称 — `addTarget()`、`newTargetId()` — 点击（代码确认）
- [ ] 快捷目标 · 恢复默认 `#targets-reset` — 调后端重置为默认列表，用返回的 targets/taxonomy 重绘 — `quick_targets_reset` — 点击（代码确认）
- [ ] 快捷目标 · 保存 `#targets-save`（primary）— 校验三条：缺 id「第 N 个目标缺少标识」、缺名称「第 N 个目标还没起名字」、tree 模式缺父节点「第 N 个目标「X」要挂节点，得先填父节点」；`res.ok!==true` 显示 `res.message`；成功用后端返回列表重绘并显示「已保存 N 个目标」 — `quick_targets_save({targets})` — 点击（代码确认）
- [ ] 快捷目标 · 未保存守卫 — 存在未保存修改时再次进入该视图只提示「有未保存的修改」，不覆盖用户编辑（`force` 才重读） — `loadTargets()`、`targetsDirty` — 自动（代码确认）
- [ ] 快捷目标 · 读取失败 — 清空列表并 `.err`「读取失败：…」 — `quick_targets_list` — 自动（代码确认）

## G. 3D 图谱 `#view-graph`（26 条）

- [ ] 图谱 · 状态行 `#graph-status` — 组合显示：「构建中…」/「加载失败」/「N 节点 / M 边 · 实时」+ 最近事件 + 同步失败原因 + 容器错误 + 「L 层 · 上限 K 节点」+ 后端 note；`title` 存 note — `updateGraphStatus()` — 自动（代码确认）
- [ ] 图谱 · 容器开关 `#graph-container-switch`（默认勾选）— 显隐大脑容器（PLY 点云）并按容器重排节点/恢复原始坐标 — `setContainerVisible()`、`containerVolume.js` — 点击（代码确认）
- [ ] 图谱 · 显示菜单按钮 `#graph-display` — 开/合 `#graph-menu`；打开时以 fixed 定位夹在窗口与 `.statusbar` 之间，避免窄窗出界；点击菜单外或窗口 `resize` 自动收起 — `openGraphMenu()`/`closeGraphMenu()` — 点击（代码确认）
- [ ] 图谱 · 菜单项「非聚焦时半透明」`#graph-idle-dim`（默认勾选）— 无聚焦时节点降透明度 — `setIdleDim()` — 点击（代码确认）
- [ ] 图谱 · 点亮层级 `#graph-levels-input`（number 1–6，默认 3）— 向邻居下探层数；`change` 时 clamp 并回写输入框，空/非法回退 3 — `applyGraphLimit()`、`setLevels()` — 输入（代码确认）
- [ ] 图谱 · 最多节点 `#graph-max-nodes`（number 1–500，默认 20）— 单次聚焦上限；整层放不下时少显示一层；clamp 回退 20 — `setMaxNodes()` — 输入（代码确认）
- [ ] 图谱 · 窄窗菜单三项 `.graph-menu-more [data-graph-action=rotate|rebuild|reload]` — 仅 ≤680px 显示（`.graph-opt-wide` 同时被 CSS 隐藏），点菜单项等价于点对应原按钮（共用状态，不分叉） — `graphMenu` click 代理 → `graphRotate/graphRebuild/graphReload.click()` — 点击（代码确认）
- [ ] 图谱 · 适配 `#graph-zoom` — 相机自适应全部节点；容器尺寸尚未就绪时 200ms 后重试 — `zoomToFit()` — 点击（代码确认）
- [ ] 图谱 · 自转 `#graph-rotate`（`.graph-opt-wide`，≤680px 隐藏）— 切换自动旋转并置 `.active`，同步菜单项高亮 — `setAutoRotate()` — 点击（代码确认）
- [ ] 图谱 · 重排 `#graph-rebuild`（`.graph-opt-wide`）— 重算 PCA 布局并重建数据；加载遮罩文案「正在重算 PCA 布局…」；失败写入状态行 — `graph_rebuild_layout` — 点击（代码确认）
- [ ] 图谱 · 重载 `#graph-reload`（`.graph-opt-wide`）— 重新拉全量快照并重启轮询 — `graph_snapshot` — 点击（代码确认）
- [ ] 图谱 · 放大/还原 `#graph-expand` — 文案在「放大/还原」间切换；调后端改窗口尺寸，120ms 后重算画布 — `set_graph_window({expanded})` — 点击（代码确认）
- [ ] 图谱 · 节点搜索 `#graph-search`（`type=search`，`autocomplete=off`）— 在已加载快照内匹配 name/id/category/tags/content_head，多词 AND；聚焦即重绘候选 — `searchGraphNodes()` — 输入/聚焦（代码确认）
- [ ] 图谱 · 搜索候选 `#graph-search-results` — 摘要「匹配 N 个图谱节点（>30 时提示先显示 30 个，请补充关键词）」/「没有匹配的图谱节点」；候选显示标题与「分类 · 标签 · id」；点击选中并把相机移近；Esc 清空搜索、Enter 选中首个、↓ 聚焦首个 — `renderGraphSearch()`、`selectNode({moveCamera})` — 点击/键盘（代码确认）
- [ ] 图谱 · 取消选择 `#graph-clear-selection` — 清除锁定与焦点 — `selectNode(null)`/`clearFocus()` — 点击（代码确认）
- [ ] 图谱 · 画布 `#graph-stage` + `#graph-canvas` — 3d-force-graph 挂载点（单独容器，避免清空覆盖层）；拖拽旋转 / 滚轮缩放 / 悬停聚焦 / 点击锁定（点击判定位移 ≤6px 且同一主键） — `GraphView._initPicking()`、`isGraphClick()` — 鼠标/触摸（代码确认）
- [ ] 图谱 · 加载遮罩 `#graph-loading` + `#graph-loading-text` — 首次「正在构建 3D 图谱（首次需读取 m3 向量，约几秒）…」，重排时改文案；结束隐藏 — `loadGraphSnapshot()` — 自动（代码确认）
- [ ] 图谱 · 空态/错误态 `#graph-empty` — 空库「记忆库还是空的 / 先在「记录」里存一条…」；失败「无法加载 3D 图谱 + 错误原因」并附「重试」按钮 — `showGraphEmpty()`、`graph_snapshot` — 自动/点击（代码确认）
- [ ] 图谱 · 节点详情 `#graph-detail` — 标题、元信息「分类 · kind · importance · 召回 N」、标签 `#tag`、正文预览（空则「（无正文预览）」）、关闭 `×`（等价取消选择） — `showGraphDetail()`/`hideGraphDetail()` — 点击/自动（代码确认）
- [ ] 图谱 · 详情「复制 id」 — `navigator.clipboard.writeText`，成功「已复制」/失败「失败」，1.2 秒复位 — 浏览器剪贴板 API — 点击（静态推断，未运行验证）
- [ ] 图谱 · 详情「按 id 搜索」 — 切到搜索视图、填入节点 id 并立即搜索 — `switchView('search')`/`runSearch()` — 点击（代码确认）
- [ ] 图谱 · 图例 `#graph-legend` — 静态说明父子/共现/大小=召回热度/操作提示；≤900px 隐藏拖拽提示、≤560px 隐藏「大小=召回热度」 — 无 — 自动（静态推断，未运行验证）
- [ ] 图谱 · 悬停提示 `.graph-node-tooltip` — 跟随指针（边界内夹取），内容为「名称 + 分类·kind·召回 + #tags」；`pointerleave/pointercancel` 隐藏 — `_label()`/`_setHover()` — 悬停（代码确认）
- [ ] 图谱 · 实时增量与轮询 — 进入即拉快照，之后每 900ms `graph_events_since({seq,qid})`；`reset` 触发全量重载，`ops` 增量应用，`has_more` 立即续拉；页面隐藏时跳过一次；连续 5 次失败停轮询并 10 秒后自动恢复 — `graph_events_since`、`pollGraph()` — 自动（代码确认）
- [ ] 图谱 · 进入/离开视图 — 进入懒建 GraphView 并激活；窗口宽 <700 且未放大时自动放大；离开停轮询、清焦点、隐藏详情并还原窗口尺寸 — `enterGraph()`/`leaveGraph()` — 自动（代码确认）
- [ ] 图谱 · 引擎渲染与容器点云 — WebGL 场景（环境光+两向光、无面板说明、关闭指针交互与节点拖拽、`cooldownTicks(0)` 固定坐标）、PLY 容器加载失败只在状态行报「容器：…」不阻塞 — `graphView.js`、`containerVolume.js`、`src/graph/assets/brain2.container.ply` — 自动（静态推断，未运行验证）

## H. 底栏与浮层面板（25 条）

- [ ] 底栏 · 自动化开关 `#auto-switch`（`label.switch-row[title=本地判档 / 判重 / 判树…]`）+ 状态 `#auto-state` — 勾选开启自动化；状态文案「手动 / 本地判档·判树 / 模型判档·判树」（模型态要求 `llm_enabled` 且 `automation_mode!=local-fallback`）；初始值来自 `get_app_config.automation_enabled` — `setAuto()` — 点击/自动（代码确认）
- [ ] 底栏 · 设置 `#settings-btn` — 开合设置面板（同时收起扩展/rsrs/更新面板）；每次打开重读系统自启真实状态 — `initSettingsPanel()` — 点击（代码确认）
- [ ] 底栏 · rsrs `#rsrs-btn` — 开合 rsrs 面板；未安装时文案变「装 rsrs」并加 `.warn`；`title` 显示版本与路径 — `initRsrsCli()`、`rsrs_cli_status` — 点击（代码确认）
- [ ] 底栏 · 更新 `#update-btn` — 已有更新信息时直接开面板，否则手动检查；检查中显示「检查中」；有更新时文案变「更新 vX」并加 `.has-update` — `checkUpdate(true)`、`update_check` — 点击（代码确认）
- [ ] 底栏 · 扩展 `#ext-btn`（`margin-left:auto` 靠右）— 开合扩展面板；打开前先取 `extension_info` 填端口/令牌，并收起其他面板；读取失败写结果区 err「无法读取扩展信息」 — `extension_info` — 点击（代码确认）
- [ ] 底栏 · 同步徽标 `#sync-badge`（默认隐藏）— 显示「离线 · N 待同步」或「N 待同步」，`.warn` 表示离线；`title` 追加历史冲突数与上次成功同步时间；无待同步项时隐藏 — `applySyncBadge()`、`runtime_status({pending,conflicts,offline,last_sync})` — 自动（代码确认）
- [ ] 底栏 · runtime 状态点 `#runtime-dot` — 在线 `.online` / 离线 `.offline`；`title` 显示「rsrs runtime 在线 (127.0.0.1:port)」或不可达原因；60 秒轮询（页面隐藏时跳过） — `checkRuntime()`、`runtime_status` — 自动（代码确认）
- [ ] 扩展面板 `#ext-panel` — 标题、端口 `#ext-port`（未启用显示「未启用」）、令牌 `#ext-token`（`type=password` readonly，占位「未生成：请重启 memTools」） — `extension_info` — 自动（代码确认）
- [ ] 扩展面板 · 安装/配对 `#ext-install`（primary）— 调用后端准备扩展目录并配对，结果写入 `#ext-install-hint`（含目录路径与「此目录含本机配对信息，请勿分享」）；执行中按钮禁用 — `extension_install` — 点击（代码确认）
- [ ] 扩展面板 · 复制令牌 `#ext-copy` — 优先 `navigator.clipboard`，失败退回 `select + execCommand('copy')`；无令牌显示「无令牌」；结果「已复制/复制失败」1.2 秒复位 — 无 — 点击（代码确认）
- [ ] 扩展面板 · 关闭 `#ext-close` 与点击面板外 — 收起面板 — 无 — 点击（代码确认）
- [ ] 扩展面板 · 默认提示 `#ext-install-hint` — 静态操作说明（打开 Chrome 开发者模式、加载目录、配对自动完成） — 无 — 自动（静态推断，未运行验证）
- [ ] rsrs 面板 `#rsrs-panel` — 标题 `#rsrs-title`、正文 `#rsrs-body`、进度条 `#rsrs-bar`/`#rsrs-bar-fill`、日志 `#rsrs-log`（空时不显示） — 无 — 自动（代码确认）
- [ ] rsrs 面板 · 关闭 `#rsrs-close` 与点击外部 — 收起面板；非重启中时复位二次确认态 — `closeRsrsPanel()` — 点击（代码确认）
- [ ] rsrs 面板 · 检查更新 `#rsrs-check` — 显示「检查中」；有新版→「发现 rsrs 新版本」并显示「更新到 vX」；已最新→「rsrs 已是最新」；失败→「检查 rsrs 更新失败」+ 错误 — `rsrs_cli_check_update` — 点击（代码确认）
- [ ] rsrs 面板 · 安装/更新 `#rsrs-install` — 用 npm/pnpm 全局安装或更新 CLI（不启动/停止 runtime）；执行期间禁用检查与安装按钮，进度条 0→100%，正文分「已安装/已是最新/版本未生效（多套 rsrs，PATH 指向另一套）」等分支 — `rsrs_cli_install`、`rsrs_cli_check_update` — 点击（代码确认）
- [ ] rsrs 面板 · 重启 rsrs（两步确认）`#rsrs-restart` — 首次点击只进入确认态（正文说明会短暂中断读写），再点「确认重启」才执行；`#rsrs-restart-cancel` 取消并恢复原文案 — `rsrs_runtime_restart` — 点击（代码确认）
- [ ] rsrs 面板 · 首次自动安装 — 启动检测到未安装且 `auto_install && can_install` 时主动亮出主窗（窗口默认隐藏）并自动开始安装；进度事件 `rsrs://install` 更新标题/正文/进度/日志 — `rsrs_cli_status`、`show_main_window`、`listen('rsrs://install')` — 自动（代码确认）
- [ ] 设置面板 `#settings-panel` — 标题、关闭 `#settings-close`、点击外部关闭 — 无 — 点击（代码确认）
- [ ] 设置面板 · 开机自启 `#autostart-switch` — 打开面板即读系统真实状态（可能被任务管理器改过）；切换调后端并回写系统状态，失败回滚开关并 `.warn` 报错；执行中禁用开关 — `autostart_status`、`autostart_set` — 点击/自动（代码确认）
- [ ] 设置面板 · 自启状态行 `#autostart-status` — 后端 message；`.ok`/`.warn` 着色；「正在开启/关闭…」 — `autostart_status` — 自动（代码确认）
- [ ] 设置面板 · rsrs runtime 行 — 文案说明「开机自启时自动检查；离线时调用 rsrs 自行拉起服务」；`#runtime-start`「立即检查」执行中禁用，结果写 `#runtime-check-status`；开机自启线程事件 `runtime://autostart` 也写该行并刷新底栏状态点 — `runtime_ensure`、`listen('runtime://autostart')` — 点击/自动（代码确认）
- [ ] 更新面板 `#update-pop` — 标题 `#update-title`、正文 `#update-body`（「尚未检查」/更新说明+当前版本+安装包大小/「正在下载 x% · a / b」/「正在重启…」）、进度条 `#update-bar`/`#update-bar-fill`、关闭 `#update-close`、点击外部关闭 — `checkUpdate()`、`fmtBytes()` — 自动（代码确认）
- [ ] 更新面板 · 下载并重启 `#update-install`（primary，默认隐藏）— 有更新时显示；点击后禁用、进度从 0 开始，成功「正在重启…」并退出进程；失败显示错误并允许重试 — `update_install`、`listen('update://progress')`、`listen('update://restarting')` — 点击（代码确认）
- [ ] 更新面板 · 启动静默检查 — 初始化时 `checkUpdate(false)`：仅更新底栏按钮，不弹面板；手动检查才弹面板；失败不打扰 — `update_check` — 自动（代码确认）

## I. 全局键盘、窗口与初始化（14 条）

- [ ] 全局键盘 · `Esc` — 隐藏主窗到托盘（任何视图，除被下层 `stopPropagation` 消费的场景，如图谱搜索框内 Esc） — `hideWindow()` → `hide_window` — 快捷键（代码确认）
- [ ] 全局键盘 · `Ctrl+Enter` / `Cmd+Enter` — 仅在记录视图提交（`currentView==='write'`），阻止默认行为 — `submit()` — 快捷键（代码确认）
- [ ] 全局键盘 · 浏览范围框 `#browse-count` `Enter` — 阻止默认并刷新 — `loadRecent()` — 快捷键（代码确认）
- [ ] 全局键盘 · 搜索框/数量框 `Enter` — 阻止默认并执行搜索 — `runSearch()` — 快捷键（代码确认）
- [ ] 全局键盘 · 图谱搜索 `Esc` / `Enter` / `ArrowDown` — 在搜索框内 Esc 清空并收起候选（`stopPropagation` 以免隐藏窗口）、Enter 选中首个候选、↓ 聚焦首个候选 — `renderGraphSearch()` — 快捷键（代码确认）
- [ ] 全局键盘 · 图谱候选列表 `ArrowUp/ArrowDown/Esc` — 在候选按钮间移动焦点、Esc 回搜索框并收起 — 无 — 快捷键（代码确认）
- [ ] 初始化 · 非 Tauri 降级 — `window.__TAURI__` 缺失时移除 `#win-grips`、在结果区显示「请在 memTools 窗口中使用」并**提前 return**（后续大部分绑定不执行，UI 骨架仍渲染） — 无 — 自动（代码确认）
- [ ] 初始化 · 幂等 — `window.__memtoolsInit` 防重复初始化（HMR） — 无 — 自动（代码确认）
- [ ] 初始化 · 启动顺序 — 事件接线 → `get_app_config` → `processor_status` → `get_taxonomy`（失败显示 err「无法读取分类树」）→ 渲染树/类型/分类 → 默认选中第一棵树 → 键盘/视图/控件绑定 → 事件监听 → `runtime_status` → 60 秒轮询 — 见 N 节 — 自动（代码确认）
- [ ] 初始化 · 调试入口 `window.__memtools` — 暴露 submit/switchView/runSearch/loadRecent/loadTargets/saveTargets/resetTargets/graph/loadGraphSnapshot/rebuildGraphLayout/pollGraph/checkUpdate/installUpdate/checkRsrsUpdate/startRsrsInstall/runProcess（不改变业务行为） — 无 — 控制台调用（代码确认）
- [ ] 窗口 · 尺寸与位置恢复 — 启动按 `config.toml [window] width/height` 设置尺寸；`remember_position` 为真时从 `window_state.json` 恢复位置与尺寸，越界夹到可见工作区，关窗/退出时保存 — `restore_window_position()`、`save_window_position()`、`window_position.rs` — 自动（静态推断，未运行验证）
- [ ] 窗口 · 全局热键 — 默认 `Ctrl+Alt+R` 注册（来自 `config.toml [hotkey] register`），按下切换主窗显隐；显示时广播 `memtools-shown`（清结果区、刷新 runtime、按视图聚焦） — `toggle_window()`、`listen('memtools-shown')` — 快捷键（代码确认）
- [ ] 窗口 · 系统托盘 — 左键点击切换主窗显隐；菜单「显示面板 / 划词悬浮（勾选项，可即时开关）/ 退出 memTools（保存位置后 exit）」；提醒扫描会更新托盘 tooltip（有到期时「memTools · N 条任务到期」） — `setup_tray()`、`selection::set_enabled()` — 点击（静态推断，未运行验证）
- [ ] 窗口 · 单实例 — 重复启动转交已有进程：带 `--kb-add` 入库并打开 AI 窗，带 `--ai` 只开 AI 窗，否则唤起主窗 — `tauri_plugin_single_instance` — 自动（静态推断，未运行验证）

## J. AI 与知识库独立窗口 `ai.html`（21 条）

- [ ] AI 窗 · 状态条 `#status-chip` — 「读取状态…」/「就绪 · agent[ · model]」/「知识库已关闭（config.toml）」/「知识库未就绪：先选目录」/错误文案；`title` 同步 — `kb_status`、`renderStatus()` — 自动（代码确认）
- [ ] AI 窗 · 知识库目录 `#btn-pick-root` — 选目录并写入知识库根目录，随后排空待处理入库队列 — `kb_pick_root`、`kb_set_root`、`kb_take_pending` — 点击（代码确认）
- [ ] AI 窗 · 新对话 `#btn-new-chat` — 重置对话（清空消息区并复位助手气泡） — `ai_reset`、`ai://event{kind:reset}` — 点击（代码确认）
- [ ] AI 窗 · 根目录显示 `#root-path` — 未设置时「未设置（点右上角「知识库目录」选择）」+ `.err`；已设置显示路径，`root_ok=false` 时 `.err` — `kb_status` — 自动（代码确认）
- [ ] AI 窗 · 添加文件夹 `#btn-add-folder` — 选文件夹入队整理 — `kb_pick_folder`、`kb_submit` — 点击（代码确认）
- [ ] AI 窗 · 右键菜单开关 `#context-menu`（`label.switch`，`title=在资源管理器右键菜单里加入「添加到 memTools 知识库」`）— 切换系统右键菜单注册；`platform_supported=false` 时禁用；失败回滚勾选并提示 — `kb_set_context_menu` — 点击（代码确认）
- [ ] AI 窗 · agent 下拉 `#agent-select` — 列出预设（可用项显示版本，未就绪项显示「未就绪」）；换来源只预览说明、清空模型候选，不立即绑定 — `agent_presets`、`loadPresets()` — 选择（代码确认）
- [ ] AI 窗 · 重新检测 `#btn-agent-refresh` — 重新探测本机 agent — `agent_presets` — 点击（代码确认）
- [ ] AI 窗 · 模型输入 `#agent-model` + 候选 `#agent-model-options`（datalist）— 可下拉选或手填；`Enter` 触发绑定；聚焦时若候选未加载则补拉 — `agent_models` — 输入/回车（代码确认）
- [ ] AI 窗 · 绑定 `#btn-agent-save` — 未就绪的预设给出明确原因并展开「安装与配置」；就绪则写回后端并提示「已绑定 X[ · 模型]」；跨来源时清空模型 — `kb_set_agent` — 点击（代码确认）
- [ ] AI 窗 · agent 提示 `#agent-hint` — 可用时「X 可用：路径 版本；模型候选 N 个（可下拉选或手填）」；未就绪时「X 未就绪：按下方「安装与配置」操作。」；模型列表失败追加原因 — `updateAgentHint()` — 自动（代码确认）
- [ ] AI 窗 · 安装与配置说明 `#agent-guide` + `#agent-guide-title` + `#agent-guide-body` — 未就绪时展示原因/安装/登录/启动/参数/已找到/文档，末行「装好后点「重新检测」，再点「绑定」。」；就绪时整块隐藏 — `renderGuide()` — 自动（代码确认）
- [ ] AI 窗 · 复制命令 `#btn-guide-copy` — 复制安装命令（无命令时退化为复制整段说明）；clipboard 不可用时用临时 textarea + `execCommand`；成功「已复制」1.2 秒复位 — 无 — 点击（代码确认）
- [ ] AI 窗 · 路径入库 `#path-input` + `#btn-add-path` — 粘贴文件/文件夹路径入队；`Enter` 等价点击 — `kb_submit` — 点击/回车（代码确认）
- [ ] AI 窗 · 网页入库 `#url-input` + `#btn-add-url` — 提交 `kind=url` 解析网页入队；`Enter` 等价点击 — `kb_submit` — 点击/回车（代码确认）
- [ ] AI 窗 · 拖放进窗口 — `enter/over` 加 `body.dragging`（虚线高亮），`drop` 逐个路径入队；无拖放 API 时静默跳过 — `webview.onDragDropEvent`、`kb_submit` — 拖放（代码确认）
- [ ] AI 窗 · 入库任务列表 `#tasks` — 每任务显示标题、状态徽标（运行中「三点+已用 N 秒」/「完成」/「失败」/阶段名）、阶段行（「处理中/已写入知识库/失败」）、`md_path|item_dir` 与 message、记忆 id；运行中每秒刷新已用时；同一 id 就地替换，新任务置顶 — `kb_submit`、`kb_tasks`、`kb://task`、`tickTasks()` — 自动（代码确认）
- [ ] AI 窗 · 对话区 `#messages` + 首屏空态 `#chat-empty` — 首次发消息即移除空态；消息按角色着色（你/AI/工具/错误）；AI 气泡含「思考中 + 已 N 秒」计时、工具活动行（去重）、流式正文；`error` 事件插错误气泡；`done` 复位忙碌；`reset` 清空 — `ai_send`、`ai://event` — 自动（代码确认）
- [ ] AI 窗 · 输入与发送 `#chat-input`（占位「…Enter 发送，Shift+Enter 换行」）+ `#btn-send`（primary）— `Enter`（无 Shift）发送；发送后清空输入、追加用户气泡并置忙碌（发送按钮禁用） — `ai_send` — 点击/回车（代码确认）
- [ ] AI 窗 · 停止 `#btn-stop`（初始 disabled）— 忙碌时可用，请求中断当前回复；失败不影响界面恢复 — `ai_abort` — 点击（代码确认）
- [ ] AI 窗 · 事件与待处理队列入库 — 监听 `ai://event`、`kb://task`、`kb://pending`；`pending` 到达即排空（`kb_take_pending` 逐条 `kb_submit`）；非 Tauri 环境状态条显示「请在 memTools 中打开」并禁用发送 — `listenEvent()`、`drainPending()` — 自动（代码确认）

## K. 划词悬浮窗 `selection.html`（14 条）

- [ ] 划词窗 · 收起态图标 `#orb`（40×40，`title/aria-label=保存选中文字到记忆`）— 点击：先取词（无文本时调 `selection_payload`），仍无文本则静默 `selection_hide` 并收起；有文本则申请展开 — `selection_payload`、`selection_expand` — 点击（代码确认）
- [ ] 划词窗 · 取词中状态 `body.reading` — 图标禁用指针并转圈，禁止重复点击 — `setReading()` — 自动（静态推断，未运行验证）
- [ ] 划词窗 · 通知脉冲 `.orb.pulse` — 收到新划词文本（未展开）时播放一次弹跳动画 — `pulseOrb()` — 自动（静态推断，未运行验证）
- [ ] 划词窗 · 展开面板 `#panel`（`role=dialog`，`aria-hidden` 随状态切换）— 展开时显示预览/状态/操作；`body.collapsed`/`body.expanded` 控制显隐；窗口尺寸由 Rust 调整 — `enterExpanded()`/`enterCollapsed()` — 自动（代码确认）
- [ ] 划词窗 · 文本预览 `#preview-text` — 有文本显示原文，无文本显示「未获取到选中文字」+ `.empty` — `renderPayload()` — 自动（代码确认）
- [ ] 划词窗 · 来源与字数 `#preview-source`（「来自 <应用>」，`title` 全量）+ `#char-count`（按码点计数「N 字」） — `renderPayload()`、`countChars()` — 自动（代码确认）
- [ ] 划词窗 · 状态行 `#status`（`role=status`，`aria-live=polite`）— 成功 `.ok` / 警告 `.warn` / 失败 `.err`；成功保存后显示「已存 · <id 前 8 位> · 当日日记」或「已返回成功，但未提供记录 ID；请在最近记忆中查询」 — `setStatus()` — 自动（代码确认）
- [ ] 划词窗 · 存为日记 `#btn-journal`（primary）— 无文本或忙碌时禁用；保存中显示「保存中…」；成功「已存为日记 ✓」且禁用，同时把「编辑」改为「查看记录」；失败恢复按钮并显示后端 message — `selection_journal({text})` — 点击（代码确认）
- [ ] 划词窗 · AI 调研 `#btn-research` — 把选中文字 `kind=text` 提交知识库队列，提示「已加入知识库队列，请在 AI 窗口查看进度」后 0.9 秒自动关闭；失败显示「提交失败：…」 — `kb_submit` — 点击（代码确认）
- [ ] 划词窗 · 编辑/查看 `#btn-edit` — 无文本禁用；未保存过时把文本送入主窗预填（`selection-prefill`），已保存过则按 memoryId 打开记录（`selection-view-memory`） — `selection_open_main({text|memoryId})` — 点击（代码确认）
- [ ] 划词窗 · 关闭 `#btn-close`（×）与 `Esc`（仅展开态、仅 Tauri）— 调后端隐藏窗口并收起；失败在状态行报错 — `selection_hide` — 点击/快捷键（代码确认）
- [ ] 划词窗 · 事件驱动状态 — `selection://show`（新划词：应用 payload、收起、脉冲）、`selection://text`（剪贴板补到文本：只更新内容与脉冲，不改变展开态）、`selection://hide`（清空文本并收起）；启动先注册监听再主动拉一次 payload — `selection_payload`、`listenEvent()` — 自动（代码确认）
- [ ] 划词窗 · 非 Tauri 降级 `body.no-tauri` — 直接展开面板、显示「请在 memTools 中打开」，四个操作按钮全部禁用 — 无 — 自动（代码确认）
- [ ] 划词窗 · 托盘总开关联动 — 托盘「划词悬浮」取消勾选时立即隐藏图标；`config.toml [selection] enabled` 决定初始勾选 — `selection::set_enabled()` — 点击（静态推断，未运行验证）

## L. 浏览器扩展对接（8 条）

- [ ] 扩展 · 本机端点信息 — 面板显示端口与令牌（来源 `{app_config_dir}/extension_token.txt`，`config.toml [extension] enabled` 控制是否启动） — `extension_info` — 自动（代码确认）
- [ ] 扩展 · 一键安装/配对 — 生成含本机配对信息的扩展目录供 Chrome「加载已解压的扩展程序」 — `extension_install` — 点击（代码确认）
- [ ] 扩展 · `GET /health` — 返回 `{ok,name,version}`，扩展据此判断应用是否在运行 — `server.rs` — 自动（静态推断，未运行验证）
- [ ] 扩展 · `GET/PUT /targets` — 读写与「快捷目标」视图同一份 `targets.json`（PUT 返回保存后的列表与 taxonomy；校验失败 400） — `targets::TargetStore` — HTTP（静态推断，未运行验证）
- [ ] 扩展 · `POST /remember` — 按快捷目标（第 1 个最优先）判档/判树后落库，返回统一响应 — `server.rs remember()`、`targets` — HTTP（静态推断，未运行验证）
- [ ] 扩展 · `POST /open` — 打开主窗并跳转到指定视图/预填内容 — `server.rs`、`selection-prefill` 同类事件 — HTTP（静态推断，未运行验证）
- [ ] 扩展 · `POST /process` — 用当前梳理来源处理文本，任务名非法返回 400「未知梳理任务：…」 — `processor::process` — HTTP（静态推断，未运行验证）
- [ ] 扩展 · `GET /agent` — 返回知识库 agent 配置/可用性（扩展侧展示） — `server.rs` — HTTP（静态推断，未运行验证）

## M. 数据持久化与配置读写（15 条）

- [ ] 配置 · `config.toml` 加载顺序 — 用户配置目录 `{config_dir}/com.memtools.desktop/config.toml` → 可执行文件同目录 → 源码目录 → `./config.toml`；缺失时用内置模板写出一份 — `config::AppConfig::load()` — 启动（代码确认）
- [ ] 配置 · `get_app_config` 字段消费 — 前端只消费 `port`（runtime 状态点文案）、`automation_enabled`、`automation_mode`、`llm_enabled`、`processor_*`；`hotkey` 与 `submit_close` 被返回但**当前 UI 未消费**（不得在重构中「补」出提交后自动关窗行为） — `get_app_config` — 启动（代码确认）
- [ ] 存储 · 快捷目标 `{app_config_dir}/targets.json` — 主窗「快捷目标」与浏览器扩展 `/targets` 共用；`quick_targets_*` 返回的 taxonomy 供父节点补全 — `targets::TargetStore` — 点击/HTTP（代码确认）
- [ ] 存储 · 梳理来源 `{app_config_dir}/processor.json` — 叠加在 `config.toml [processor]` 之上，界面点选即写入，重启后保持 — `processor_settings::ProcessorSettings` — 点击（代码确认）
- [ ] 存储 · 知识库设置 `{app_config_dir}/knowledge_base.json` — 根目录、agent 绑定、右键菜单开关 — `kb_settings::KbSettings`、`kb_set_root`、`kb_set_agent`、`kb_set_context_menu` — 点击（代码确认）
- [ ] 存储 · 扩展令牌 `{app_config_dir}/extension_token.txt` — 固定令牌留空时首启自动生成并写盘 — `extension_info`、`server::start` — 启动（代码确认）
- [ ] 存储 · 窗口位置 `{app_config_dir}/window_state.json` — 记录坐标与尺寸（兼容旧格式仅坐标）；托盘退出与显示前保存 — `save_window_position()`/`restore_window_position()` — 自动（代码确认）
- [ ] 存储 · 图谱缓存目录 `{app_config_dir}/graph/` — 图谱服务缓存（m3 向量/PCA 结果），`graph_rebuild_layout` 会重写 — `graph::GraphService` — 自动（静态推断，未运行验证）
- [ ] 存储 · 提醒扫描缓存 `{app_config_dir}/reminders/` — 后台线程每 `interval_minutes` 扫描一次并缓存「忽略」状态 — `reminders::scan` — 自动（静态推断，未运行验证）
- [ ] 存储 · 自动化备份目录 `{app_config_dir}/backups/` — 「改/并」前备份原文（提醒「完成」也会先备份） — `automation_backup_dir()`、`reminder_complete` — 自动（代码确认）
- [ ] 存储 · 记忆正文真相源 — 记忆读写全部走 rsrs runtime（`remember`/`recall`/`list`/`show`/`attach`/`update`），本地不落正文 — `rsrs_client.rs` — 自动（静态推断，未运行验证）
- [ ] 缓存 · 全量记忆目录 `memoryCatalog`（内存）— 首次进浏览/搜索/上级搜索拉一次 `list_recent({limit:0,category:null})`；写入成功后 `browseDirty=true` + `catalogRevision++` 触发下次重取；`truncated` 视为未读全，报错让用户刷新 — `ensureMemoryCatalog()` — 自动（代码确认）
- [ ] 缓存 · 折叠与请求序号 `browseCollapsed`/`searchCollapsed`/`browseReq`/`searchReq`/`remindReq`/`graphCursor` — 折叠状态按视图独立、请求序号丢弃过期响应 — 无量 — 自动（代码确认）
- [ ] 外观 · 现有主题能力 — 仅 `@media (prefers-color-scheme: light/dark)` 两套令牌自动跟随系统；**无**主题切换控件、无 `localStorage` 主题持久化（`src/theme/` 下的 engine/presets/tokens 为未跟踪、未被任何 HTML/JS/CSS 引用，属未接线模块，不计入既有功能） — `styles.css:1118`、`styles.css:1784` — 自动（代码确认）
- [ ] 外观 · 无边框透明窗口 — `tauri.conf.json` 主窗 `decorations:false, transparent:true, alwaysOnTop:true, skipTaskbar:true`，尺寸由配置与拖拽缩放决定 — 无 — 自动（静态推断，未运行验证）

## N. Tauri 命令调用点与前置条件总表（61 条注册，57 条被前端调用，4 条注册未接线）

| # | 命令 | 调用点 | 前置条件 / 备注 |
|---|------|--------|-----------------|
| 1 | `get_taxonomy` | main.js:2606 | Tauri 运行时；失败写结果区 err |
| 2 | `submit_memory` | main.js:1700 | 手动模式；主区需树/上级；正文非空 ≤8000 字 |
| 3 | `get_app_config` | main.js:2598 | 启动；失败静默用默认值 |
| 4 | `extension_info` | main.js:1979 | 点「扩展」；失败写结果区 |
| 5 | `extension_install` | main.js:1998 | 点「安装 / 配对」；执行中禁用 |
| 6 | `runtime_status` | main.js:2947 | 启动、每 60s、窗口显示、操作后 |
| 7 | `hide_window` | main.js:1905 | — / ✕ / Esc |
| 8 | `quit_app` | 无前端调用 | 注册未用；退出实际走托盘 `app.exit(0)` |
| 9 | `selection_payload` | selection.js:168,424 | 点图标取词 / 启动补拉 |
| 10 | `selection_expand` | selection.js:238 | 有文本后申请展开；失败转前端展开并报错 |
| 11 | `selection_hide` | selection.js:228,357 | 取词为空 / 点 × / Esc |
| 12 | `selection_open_main` | selection.js:338 | 点「编辑/查看记录」 |
| 13 | `selection_journal` | selection.js:271 | 有文本；`ok=false` 时报 message |
| 14 | `selection_targets` | 无前端调用 | 注册未用（扩展走 HTTP `/targets`） |
| 15 | `selection_save_target` | 无前端调用 | 注册未用 |
| 16 | `quick_targets_list` | main.js:1296 | 进快捷目标视图且无未保存修改 |
| 17 | `quick_targets_save` | main.js:1368 | 三条前端校验通过 |
| 18 | `quick_targets_reset` | main.js:1388 | 点「恢复默认」 |
| 19 | `automation_submit` | main.js:1694 | 自动化开启；正文非空 ≤8000 字 |
| 20 | `automation_resolve` | main.js:1890 | 择一（改/并/挂需勾选候选） |
| 21 | `process_memory` | main.js:1655 | 梳理开启、正文非空 ≤8000 字 |
| 22 | `processor_status` | main.js:1566 | 启动 / 展开梳理条 |
| 23 | `processor_set_source` | main.js:1582 | 未在切换中且目标来源不同 |
| 24 | `processor_reset_source` | 无前端调用 | 注册未用 |
| 25 | `processor_probe` | main.js:1609 | 切到 agent 来源后自动探测 |
| 26 | `list_recent` | main.js:667 | 浏览/搜索/上级搜索首次或失效后 |
| 27 | `search_memory` | main.js:897 | 语义模式；limit 夹在 1..100 |
| 28 | `memory_path` | main.js:1784 | 落库回执且有 id |
| 29 | `memory_outline` | main.js:1792 | 回执 `branch=attach` 且有 parent |
| 30 | `list_reminders` | main.js:953 | 进提醒视图 / 刷新 / 到期事件 |
| 31 | `reminder_dismiss` | main.js:1065 | 点「忽略」；带 id+due |
| 32 | `reminder_complete` | main.js:1045 | 点「完成」；后端先备份再追加【完成】 |
| 33 | `graph_snapshot` | main.js:413 | 首次进图谱 / 重载 / 重试 |
| 34 | `graph_events_since` | main.js:481 | 图谱可见且已加载、页面未隐藏 |
| 35 | `graph_rebuild_layout` | main.js:641 | 点「重排」且无进行中的加载 |
| 36 | `set_graph_window` | main.js:397 | 放大/还原；进入图谱且宽 <700 时自动放大 |
| 37 | `update_check` | main.js:2507 | 启动静默一次 + 点「更新」 |
| 38 | `update_install` | main.js:2552 | 有更新且未在安装中 |
| 39 | `rsrs_cli_status` | main.js:2301,2346 | 启动 / 重启完成后 |
| 40 | `rsrs_cli_check_update` | main.js:2135,2201 | 安装后回查 / 点「检查更新」 |
| 41 | `rsrs_cli_install` | main.js:2126 | 点安装/更新；`rsrsBusy` 时拒绝 |
| 42 | `autostart_status` | main.js:2396 | 打开设置面板 |
| 43 | `autostart_set` | main.js:2411 | 切换自启开关；失败回滚 |
| 44 | `runtime_ensure` | main.js:2429 | 点「立即检查」 |
| 45 | `rsrs_runtime_restart` | main.js:2286 | 二次确认后；执行中禁用按钮 |
| 46 | `show_main_window` | main.js:2356 | 首启自动安装前亮窗 |
| 47 | `kb_status` | ai.js:765 | AI 窗启动 |
| 48 | `kb_set_root` | ai.js:607 | 选到目录后 |
| 49 | `kb_pick_root` | ai.js:605 | 点「知识库目录」 |
| 50 | `kb_pick_folder` | ai.js:617 | 点「添加文件夹」 |
| 51 | `kb_set_context_menu` | ai.js:643 | 切换右键菜单；失败回滚 |
| 52 | `kb_submit` | ai.js:581；selection.js:312 | 各类入库 / 划词 AI 调研 |
| 53 | `kb_tasks` | ai.js:772 | AI 窗启动补齐 |
| 54 | `kb_take_pending` | ai.js:592 | 启动与收到 `kb://pending` |
| 55 | `kb_set_agent` | ai.js:306 | 预设就绪时点「绑定」 |
| 56 | `agent_presets` | ai.js:256 | 启动 / 重新检测 |
| 57 | `agent_models` | ai.js:235 | 换来源 / 聚焦模型框且缓存失效 |
| 58 | `ai_send` | ai.js:515 | 有文本且非忙碌 |
| 59 | `ai_abort` | ai.js:525 | 忙碌时可点 |
| 60 | `ai_reset` | ai.js:692 | 点「新对话」 |
| 61 | `ai_open` | main.js:2858 | 点顶栏「AI」；失败写结果区 err |

## O. 跨窗口事件通道总表

| 事件 | 发出方 | 监听方 | 作用 |
|------|--------|--------|------|
| `memtools-shown` | lib.rs `show_window()` | main.js | 清结果区、刷新 runtime、按视图恢复焦点 |
| `selection-prefill` | lib.rs `selection_open_main` | main.js | 主窗切记录视图并预填选中文字 |
| `selection-view-memory` | lib.rs `selection_open_main` | main.js | 主窗按 id 打开记录（校验 id 形状） |
| `selection://show` | selection.rs | selection.js | 新划词 payload、收起并脉冲 |
| `selection://text` | selection.rs | selection.js | 剪贴板补文本，不改变展开态 |
| `selection://hide` | selection.rs | selection.js | 清空文本并收起 |
| `reminders://due` | lib.rs 提醒线程 | main.js | 更新 tab 徽标；在提醒视图时重扫 |
| `rsrs://install` | lib.rs `rsrs_cli_install` | main.js | 安装/更新进度、阶段、日志 |
| `update://progress` | updater.rs | main.js | 下载进度条与百分比 |
| `update://restarting` | lib.rs `update_install` | main.js | 正文「正在重启…」 |
| `runtime://autostart` | lib.rs 自启线程 | main.js | 设置面板状态行 + 底栏状态点 |
| `ai://event` | lib.rs `ai_send` 流 | ai.js | start/text/tool/error/done/reset 流式渲染 |
| `kb://task` | kb_service.rs | ai.js | 入库任务进度与结果 |
| `kb://pending` | kb_service.rs | ai.js | 触发排空待处理队列 |

## P. 高风险的既有行为（12 条，重构易破坏，逐条对照）

- [ ] 风险 1 · `.hidden { display:none !important }` 是全局显隐开关，视图/层级/浮层/空态全依赖它；改成 `display:flex` 或去掉 `!important` 会让空态与面板常驻显示 — styles.css:38（代码确认）
- [ ] 风险 2 · 窄窗把图谱三个次要动作**用 CSS 移进菜单**（`.graph-opt-wide{display:none}` + `.graph-menu-more{display:flex}`，断点 680px），并靠 `data-graph-action` 代理点击原按钮；丢掉任一侧就有按钮消失或双份 — styles.css:1861,1903；main.js:2775（代码确认）
- [ ] 风险 3 · 无 Tauri 守卫「提前 return」在键盘/视图绑定之前，仅 `#browse-source` 与 `#parent-query` 在守卫前绑定；改写初始化顺序会让「非 Tauri 提示」变成死界面或让部分控件静默失效 — main.js:2564-2583（代码确认）
- [ ] 风险 4 · 三层去向的状态机是**互相清空**关系：选树清 `selectedParent`、选上级清树 chips、切日记隐藏层二三、开自动化隐藏层一；文案由 `updateDestination()` 单点生成，任何一处改为局部更新都会出现「去向显示与实际提交不一致」 — main.js:300-306,1405-1466（代码确认）
- [ ] 风险 5 · 触发行显隐由「手动 + 主区 + task + 正文无【触发】」四条件 + `forceShow` 共同决定，且 `submit_memory` 会在 `needs_trigger` 时二次要求；合并成一次判断会丢掉「后端要求补日期」分支 — main.js:259-265,1711-1714（代码确认）
- [ ] 风险 6 · 记忆树/列表的**祖先补全**依赖 `memoryForest()`（只补命中项祖先、环安全、路径节点不计入命中）与 `.path-context` 样式；换成普通扁平树会改变「匹配 N 条」语义并丢虚线路径视觉 — src/memory-browser.js、styles.css:1781（代码确认）
- [ ] 风险 7 · 正文片段定位依赖 `[hh:mm]` 分节正则与 UTF-16 偏移，且高亮用 `<mark>` + `textContent` 拼接（防 XSS）；改成 `innerHTML` 拼接会引入注入风险并让「查看原文」滚动定位偏移 — src/diary-fragments.js `matchingFragments`、main.js:836-864（代码确认）
- [ ] 风险 8 · 图谱详情/搜索/菜单都靠 DOM 结构与 `data-*` 约定：`#graph-menu` 需 fixed 夹取定位（避开 `.statusbar`）、候选列表用 `querySelector('button')` 取首项、`.graph-search-field` 外点击才收起；改类名或嵌套层级会同时破坏定位与键盘操作 — main.js:2739-2791（代码确认）
- [ ] 风险 9 · 缩放手柄与拖拽区是**功能性命中区**（`.win-grips{pointer-events:none}` + `.win-grip{pointer-events:auto}`，边 6px/角 12px，`inset:0` 且 z-index:60），视觉改造若改 padding/z-index 会挡住内容或让窗口无法缩放；且缺 `startResizeDragging` 时脚本会移除整块手柄 — styles.css:1831-1853、main.js:1936-1965（代码确认）
- [ ] 风险 10 · 弹层互斥与「点外部关闭」是四份独立 document 监听（扩展/rsrs/设置/更新），都靠 `contains(e.target) || btn.contains(e.target)` 判定；合并成一份若漏掉某个容器会出现「点面板内部即被关掉」 — main.js:2028,2338,2454,2821（代码确认）
- [ ] 风险 11 · 请求竞态靠序号（`browseReq`/`searchReq`/`remindReq`/`catalogRevision`）与 `browseDirty` 失效标记；抽组件时若改成直接 await 重绘，快速切分类/搜索会出现旧响应覆盖新结果 — main.js:144,156-157,660-679,882-919（代码确认）
- [ ] 风险 12 · 划词窗尺寸/位置由 Rust 侧 `apply_rect` 按 DPI 调整，前端只切 `body.collapsed/expanded` 与 `aria-hidden`；把展开尺寸写进 CSS 会与后端尺寸冲突（面板被裁切或点不到按钮） — selection.js:128-156、selection.rs `selector_size_at`（代码确认）

## Q. 分项计数（与各节标题一致，自动统计）

| 分区 | 条目数 |
|------|--------|
| A 外壳 · 顶栏 · 视图切换 | 7 |
| B 记录视图 `#view-write` | 24 |
| C 最近记忆 `#view-browse` | 12 |
| D 搜索 `#view-search` | 8 |
| E 提醒 `#view-remind` | 8 |
| F 快捷目标 `#view-targets` | 14 |
| G 3D 图谱 `#view-graph` | 26 |
| H 底栏与浮层面板 | 25 |
| I 全局键盘、窗口与初始化 | 14 |
| J AI 与知识库窗 `ai.html` | 21 |
| K 划词悬浮窗 `selection.html` | 14 |
| L 浏览器扩展对接 | 8 |
| M 数据持久化 / 配置 / 外观 | 15 |
| **功能清单合计** | **196** |
| P 高风险既有行为（独立计数） | 12 |
| **全文勾选项合计** | **208** |

另：N 节覆盖 61 条 Tauri 命令（57 条有前端调用点、4 条仅注册未接线：`quit_app`/`processor_reset_source`/`selection_targets`/`selection_save_target`），O 节覆盖 14 条跨窗口事件。

标注统计（按勾选条目行计，互斥不重复）：`（代码确认）` 185 条；`（静态推断，未运行验证）` 23 条；两者相加等于全文 208 条。P 节 12 条风险项标注 `（代码确认）`。全清单未在运行中的应用窗口实测，`（代码确认）` 仅表示绑定与分支可逐行核对。

控件 id 落位：`index.html`（HEAD 版）共 125 个唯一 id，其中 51 个是带 id 的 button/input/select/textarea，其余为视图容器、状态行、datalist 与 `#win-grips` 手柄；另有 `.view-tab`/`.mode-btn`/`.seg-btn`/`.process-task`/`.type-pill`/`.tree-chip`/`.win-grip` 等无 id 的类选择器控件（A–H 节按类名标注）。`ai.html` 26 个 id、`selection.html` 10 个 id 分别落在 J、K 节。
