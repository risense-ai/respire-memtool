/**
 * 主题预设：四套内置主题（唯一真源）
 *
 * 每套主题 = 令牌层(tokens) + 布局层(layout)。
 * 令牌层开放：色彩 / 字体 / 形状 / 密度；布局层开放：分栏模式 / 栏宽 / 区域显隐 / 面板次序。
 * 组件样式只允许引用令牌，不得写死色值（见 design/spec/theme-engine.md）。
 */

/** 所有主题共享的结构性默认值；各预设只需覆盖差异项。 */
export const BASE = {
  meta: { version: 1, author: "memTools" },
  layout: {
    id: "right-rail",
    regions: {
      order: ["header", "tabs", "main", "status"],
      visible: { header: true, tabs: true, status: true },
      size: { header: "44px", tabs: "36px", status: "36px" },
    },
    main: {
      // ["split", 左栏宽, 右栏宽] | ["stack"]
      mode: ["split", "1fr", "188px"],
      // 面板次序：可互换
      order: ["input", "target"],
      gap: "12px",
    },
    responsive: {
      // 窗口宽度低于此值时 split 自动降级为 stack，避免 480 小窗被压坏
      stackBelow: 460,
      wideAbove: 960,
    },
  },
};

const FONT_UI =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", "Helvetica Neue", Arial, sans-serif';
const FONT_SERIF = '"Noto Serif SC", "Songti SC", Georgia, "Times New Roman", serif';
const FONT_MONO = '"JetBrains Mono", "Cascadia Code", Consolas, monospace';

/** 深合并：预设值覆盖 BASE，对象递归、数组整体替换。 */
export function mergeTheme(base, patch, path = "", sink = []) {
  const out = Array.isArray(base) ? [...base] : { ...base };
  if (!patch || typeof patch !== "object") return out;
  for (const key of Object.keys(patch)) {
    const cur = out[key];
    const next = patch[key];
    const where = path ? `${path}.${key}` : key;
    if (Array.isArray(next)) {
      out[key] = [...next];
    } else if (next && typeof next === "object") {
      out[key] = cur && typeof cur === "object" && !Array.isArray(cur)
        ? mergeTheme(cur, next, where, sink)
        : mergeTheme({}, next, where, sink);
    } else {
      out[key] = next;
    }
  }
  return out;
}

/** 四套内置主题 */
export const PRESETS = [
  {
    id: "precision",
    name: "精工极简",
    desc: "白底冷灰，克制的蓝；原生面板感",
    tokens: {
      color: {
        bg: { base: "#FFFFFF", raised: "#FFFFFF", sunken: "#F7F8FA" },
        line: { base: "#E5E8ED", soft: "#EFF1F4" },
        text: { primary: "#17191E", secondary: "#59616D", faint: "#89919E" },
        accent: { base: "#2F6FEB", on: "#FFFFFF", soft: "#F1F5FE" },
        state: {
          success: "#1B7F4B", successSoft: "#E9F6EF",
          warning: "#9A6100", warningSoft: "#FCF3E4",
          danger: "#C0392B", dangerSoft: "#FBEBE9",
        },
      },
      typography: {
        fontUi: FONT_UI, fontBrand: FONT_UI, fontMono: FONT_MONO,
        scale: { label: "11px", body: "13px", title: "20px" },
        weight: { regular: 400, medium: 500, strong: 600 },
        letter: { tight: "-0.011em", normal: "0", wide: "0.06em" },
        line: { tight: 1.25, normal: 1.5 },
      },
      shape: {
        radius: { none: "0px", sm: "4px", md: "6px", lg: "10px" },
        border: { width: "1px" },
        shadow: { card: "0 1px 2px rgba(23,25,30,.06)", pop: "0 10px 28px rgba(23,25,30,.14)" },
      },
      density: {
        space: { xs: "4px", sm: "8px", md: "12px", lg: "16px", xl: "24px" },
        control: { height: "28px", gap: "8px" },
      },
    },
  },
  {
    id: "swiss",
    name: "瑞士网格",
    desc: "纯白黑字，朱红一点；强网格与发丝线",
    tokens: {
      color: {
        bg: { base: "#FFFFFF", raised: "#FFFFFF", sunken: "#F5F5F5" },
        line: { base: "#111111", soft: "#E4E4E4" },
        text: { primary: "#111111", secondary: "#6B6B6B", faint: "#919191" },
        accent: { base: "#E32B1B", on: "#FFFFFF", soft: "#FDECEA" },
        state: {
          success: "#146B3A", successSoft: "#E7F4EC",
          warning: "#8A4B00", warningSoft: "#FBF1E2",
          danger: "#C4160A", dangerSoft: "#FBE9E7",
        },
      },
      typography: {
        fontUi: FONT_UI, fontBrand: FONT_UI, fontMono: FONT_MONO,
        scale: { label: "10px", body: "13px", title: "22px" },
        weight: { regular: 400, medium: 500, strong: 700 },
        letter: { tight: "-0.02em", normal: "0", wide: "0.08em" },
        line: { tight: 1.2, normal: 1.45 },
      },
      shape: {
        radius: { none: "0px", sm: "0px", md: "2px", lg: "2px" },
        border: { width: "1px" },
        shadow: { card: "none", pop: "0 8px 24px rgba(0,0,0,.14)" },
      },
      density: {
        space: { xs: "4px", sm: "8px", md: "12px", lg: "16px", xl: "24px" },
        control: { height: "28px", gap: "8px" },
      },
    },
  },
  {
    id: "paper",
    name: "纸感编辑",
    desc: "暖米白纸底，墨色与赭石；书卷气",
    tokens: {
      color: {
        bg: { base: "#FAF8F3", raised: "#FFFDF8", sunken: "#F2EEE4" },
        line: { base: "#E2DCCE", soft: "#EDE8DC" },
        text: { primary: "#2A2621", secondary: "#6B6255", faint: "#968B7C" },
        accent: { base: "#A2562B", on: "#FFFFFF", soft: "#F6EDE6" },
        state: {
          success: "#3C6B4A", successSoft: "#EAF0EA",
          warning: "#8A5A12", warningSoft: "#F7EFDF",
          danger: "#A33A2A", dangerSoft: "#F7E8E4",
        },
      },
      typography: {
        fontUi: FONT_UI, fontBrand: FONT_SERIF, fontMono: FONT_MONO,
        scale: { label: "11px", body: "13px", title: "21px" },
        weight: { regular: 400, medium: 500, strong: 600 },
        letter: { tight: "-0.005em", normal: "0", wide: "0.05em" },
        line: { tight: 1.3, normal: 1.6 },
      },
      shape: {
        radius: { none: "0px", sm: "3px", md: "3px", lg: "4px" },
        border: { width: "1px" },
        shadow: { card: "none", pop: "0 10px 26px rgba(60,50,35,.16)" },
      },
      density: {
        space: { xs: "4px", sm: "8px", md: "12px", lg: "16px", xl: "24px" },
        control: { height: "30px", gap: "8px" },
      },
    },
  },
  {
    id: "calm",
    name: "温润器具",
    desc: "灰白素面，深绿一点；无装饰大留白",
    tokens: {
      color: {
        bg: { base: "#F6F6F4", raised: "#FFFFFF", sunken: "#EFEFEC" },
        line: { base: "#DCDCD7", soft: "#E8E8E4" },
        text: { primary: "#2A2926", secondary: "#6B6963", faint: "#8C8A81" },
        accent: { base: "#2E4F45", on: "#FFFFFF", soft: "#E7EDE9" },
        state: {
          success: "#2E4F45", successSoft: "#E7EDE9",
          warning: "#8A6414", warningSoft: "#F4EFE2",
          danger: "#A6402F", dangerSoft: "#F4E9E6",
        },
      },
      typography: {
        fontUi: FONT_UI, fontBrand: FONT_UI, fontMono: FONT_MONO,
        scale: { label: "11px", body: "13px", title: "19px" },
        weight: { regular: 300, medium: 400, strong: 500 },
        letter: { tight: "0", normal: "0", wide: "0.04em" },
        line: { tight: 1.35, normal: 1.6 },
      },
      shape: {
        radius: { none: "0px", sm: "0px", md: "2px", lg: "2px" },
        border: { width: "1px" },
        shadow: { card: "none", pop: "0 8px 22px rgba(42,41,38,.12)" },
      },
      density: {
        space: { xs: "4px", sm: "8px", md: "12px", lg: "16px", xl: "24px" },
        control: { height: "30px", gap: "10px" },
      },
    },
  },
];

export const DEFAULT_THEME_ID = "precision";

/** 取完整主题（BASE 深合并预设覆盖层）。 */
export function resolveTheme(preset, overrides = {}) {
  return mergeTheme(mergeTheme(structuredClone(BASE), preset.tokens ? { tokens: preset.tokens } : {}, "tokens"), overrides);
}

/** 按 id 找预设。 */
export function findPreset(id) {
  return PRESETS.find((p) => p.id === id) || PRESETS.find((p) => p.id === DEFAULT_THEME_ID);
}
