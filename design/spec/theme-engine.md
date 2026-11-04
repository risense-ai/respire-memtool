# memTools 主题引擎规格 v1

> 目标：一套布局 + 可配置令牌 + 可配置布局，内置四套主题，用户可改可导入导出。
> 硬约束：**功能零缺失**（改造前后逐项对照 `functional-parity.md`）；业务逻辑（Tauri 命令、状态机、存储）不改。

---

## 1. 分层架构

```
theme JSON  ──►  ThemeEngine  ──►  CSS 变量(:root)  +  data-layout 属性
                                        │
        ┌───────────────────────────────┼──────────────────────────────┐
        ▼                               ▼                              ▼
  theme/tokens.css              theme/layout.css                 styles.css
  （只有变量声明，        （网格与区域骨架，           （组件规则，只引用
    四套预设在此）           响应式断点）                 令牌，禁止裸色值）
```

**铁律**：`styles.css` 内不得出现 `#hex` / `rgb()` / `rgba()` 字面量（透明度用令牌派生），
只允许 `var(--*)`。新增组件必须先从令牌取值。

---

## 2. 主题 JSON 契约

```jsonc
{
  "id": "swiss",                    // 唯一标识，用于 data-theme
  "name": "瑞士网格",                // 面板显示名
  "meta": { "version": 1, "author": "" },

  "tokens": {
    "color": {
      "bg":    { "base": "#FFFFFF", "raised": "#FFFFFF", "sunken": "#F5F5F5" },
      "line":  { "base": "#111111", "soft": "#E4E4E4" },
      "text":  { "primary": "#111111", "secondary": "#767676", "faint": "#A0A0A0" },
      "accent":{ "base": "#E32B1B", "on": "#FFFFFF", "soft": "#FDECEA" },
      "state": { "success": "#1F7A4D", "warning": "#A15C00", "danger": "#C62828",
                 "successSoft": "#E8F5EE", "warningSoft": "#FDF3E3", "dangerSoft": "#FBEAEA" }
    },
    "typography": {
      "fontUi":    "-apple-system, \"PingFang SC\", sans-serif",
      "fontBrand": "\"Noto Serif SC\", Georgia, serif",   // 可与 fontUi 相同
      "fontMono":  "\"JetBrains Mono\", Consolas, monospace",
      "scale":  { "label": "10px", "body": "13px", "title": "22px" },
      "weight": { "regular": 400, "medium": 500, "strong": 700 },
      "letter": { "tight": "-0.01em", "normal": "0", "wide": "0.06em" },
      "line":   { "tight": 1.25, "normal": 1.5 }
    },
    "shape": {
      "radius": { "none": "0px", "sm": "2px", "md": "4px", "lg": "8px" },
      "border": { "width": "1px" },
      "shadow": { "card": "none", "pop": "0 8px 24px rgba(0,0,0,.12)" }
    },
    "density": {
      "space":   { "xs": "4px", "sm": "8px", "md": "12px", "lg": "16px", "xl": "24px" },
      "control": { "height": "28px", "gap": "8px" }
    }
  },

  "layout": {
    "id": "right-rail",
    "regions": {
      "order":   ["header", "tabs", "main", "status"],
      "visible": { "header": true, "tabs": true, "status": true },
      "size":    { "header": "44px", "tabs": "36px", "status": "36px" }
    },
    "main": {
      "mode": ["split", "1fr", "188px"],   // ["split", colA, colB] | ["stack"]
      "order": ["input", "target"],        // 面板次序，可互换
      "gap": "12px"
    },
    "responsive": {
      "stackBelow": 460,                   // 窗口窄于此值：split 自动降级为 stack
      "wideAbove": 960
    }
  }
}
```

### 令牌命名映射（JSON → CSS 变量）

| JSON 路径 | CSS 变量 |
|---|---|
| `color.bg.base` | `--c-bg` |
| `color.bg.raised` | `--c-bg-raised` |
| `color.bg.sunken` | `--c-bg-sunken` |
| `color.line.base` / `.soft` | `--c-line` / `--c-line-soft` |
| `color.text.primary` / `.secondary` / `.faint` | `--c-text` / `--c-text-dim` / `--c-text-faint` |
| `color.accent.base` / `.on` / `.soft` | `--c-accent` / `--c-on-accent` / `--c-accent-soft` |
| `color.state.*` | `--c-success` / `--c-warning` / `--c-danger` / `--c-success-soft` … |
| `typography.fontUi` / `fontBrand` / `fontMono` | `--font` / `--font-brand` / `--font-mono` |
| `typography.scale.label` / `body` / `title` | `--fs-label` / `--fs-body` / `--fs-title` |
| `typography.weight.*` | `--fw-regular` / `--fw-medium` / `--fw-strong` |
| `typography.letter.*` | `--ls-tight` / `--ls-normal` / `--ls-wide` |
| `typography.line.*` | `--lh-tight` / `--lh-normal` |
| `shape.radius.*` | `--r-none` / `--r-sm` / `--r-md` / `--r-lg` |
| `shape.border.width` | `--bw` |
| `shape.shadow.card` / `.pop` | `--sh-card` / `--sh-pop` |
| `density.space.*` | `--sp-xs` … `--sp-xl` |
| `density.control.height` / `.gap` | `--ctl-h` / `--ctl-gap` |
| `layout.*` | `--layout-*`（区域高度、分栏宽度、间距） |

**兼容旧令牌**：`--bg-base / --border / --text / --text-dim / --accent / --radius / --shadow`
保留为别名（指向新令牌），避免 183 条规则一次性改写出现漏改。

---

## 3. 布局开放模型

布局用 `data-layout` 属性 + CSS 变量驱动，**不写死结构**：

```html
<body data-theme="swiss" data-layout="right-rail" data-target-order="after">
```

`layout.css` 负责：
1. `#app` 为纵向 flex；`regions.order` 决定 header/tabs/main/status 的 DOM 次序（由 JS 按配置重排，非 CSS 硬编码）。
2. `main.mode = ["split", a, b]` → `display:grid; grid-template-columns: var(--layout-col-a) var(--layout-col-b)`。
3. `main.mode = ["stack"]` → `grid-template-columns: 1fr`，面板纵向堆叠。
4. `main.order` 决定 input / target 谁在前（CSS `order` 属性）。
5. `stackBelow` → 媒体查询降级：宽度小于该值时 split 转 stack（避免 480 窗被压坏）。
6. `regions.visible[region]=false` → 该区域 `display:none`。

**开放边界（防呆）**：栏宽下限 140px、上限 45%；`stackBelow` 最小 360px（Tauri minWidth）。
越界值由 ThemeEngine 夹紧并记录 warning，不静默接受。

---

## 4. 四套内置主题

| id | 名称 | 底色 | 主文 | 强调 | 字体 | 圆角 | 阴影 |
|---|---|---|---|---|---|---|---|
| `swiss` | 瑞士网格 | `#FFFFFF` | `#111111` | `#E32B1B` 朱红 | 无衬线 / 品牌同 | 0–2px | 无 |
| `paper` | 纸感编辑 | `#FAF8F3` | `#2A2621` | `#A2562B` 赭石 | 界面无衬线 / 品牌衬线 | 3px | 无 |
| `precision` | 精工极简 | `#FFFFFF` | `#17191E` | `#2F6FEB` 蓝 | 无衬线 / 品牌同 | 4–8px | 极轻 |
| `calm` | 温润器具 | `#F6F6F4` | `#2A2926` | `#2E4F45` 深绿 | 细字重无衬线 | 0–2px | 无 |

默认主题：`precision`（最通用、对比度安全）。

---

## 5. 主题面板与持久化

**入口**：设置面板（`#settings-panel`）内新增「外观」分区，与现有「开机自启 / rsrs runtime」并列。

**面板能力**：
- 预设选择：四套内置主题卡片，点选即预览
- 令牌编辑：色彩（取色器 + 十六进制输入）、字体（下拉：系统无衬线/衬线/等宽栈）、
  形状（圆角滑块 0–12px、边框粗细 1/2px）、密度（间距基准 4/8/12px 三档、控件高度滑块）
- 布局编辑：分栏模式（左右分栏 / 纵向堆叠）、分栏比例滑块、区域显隐开关、面板次序
- 操作：重置为当前预设 / 导出 JSON（复制到剪贴板 + 下载文件）/ 导入 JSON（粘贴 + 选文件）
- 校验：导入的 JSON 缺字段用默认值补全；非法色值拒绝并提示具体字段

**持久化**：
- `localStorage["memtool.theme"]` = 完整主题 JSON（含用户改动）
- `localStorage["memtool.themeId"]` = 当前预设 id
- 启动时读取并应用；无则用默认 `precision`
- 每套预设的原始 JSON 只读存在于 `tokens.css` / `presets.js`，用户改动覆盖层单独存

---

## 6. 验收标准

1. **构建**：`npm run build` 零错误；`cargo check`（若触及 Rust）零错误
2. **零缺失**：`functional-parity.md` 清单逐项勾选通过，无一项丢失
3. **令牌纯净**：`grep -nE '#[0-9a-fA-F]{3,8}|rgba?\(' src/styles.css` 结果为空（组件层）
4. **四套主题**：切换四套均无错位、无对比度不足（正文对比度 ≥ 4.5:1，次要文字 ≥ 3:1）
5. **布局开放**：split/stack 切换、栏宽调整、区域显隐、面板次序互换均生效且 480×560 不溢出
6. **导入导出**：导出的 JSON 能原样导回并还原外观
7. **持久化**：重启应用后主题保持
8. **文案**：`copy-rewrite.md` 全部替换生效，界面无 emoji、无感叹号、无语气词

---

## 7. 实施顺序（有依赖，须串行）

1. `theme/presets.js` + `theme/tokens.css`（四套预设 → CSS 变量）
2. `theme/layout.css`（区域骨架 + 响应式）
3. `theme/engine.js`（应用/校验/持久化/导入导出）
4. `styles.css` 令牌化（193 处硬编码色值 → var）
5. HTML 结构调整（右栏 23 纲常驻、删除染色渐变/毛玻璃）
6. 主题面板 UI
7. 文案替换
8. 逐项验收
