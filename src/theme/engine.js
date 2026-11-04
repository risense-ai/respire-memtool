/**
 * ThemeEngine：把主题 JSON 应用为 CSS 变量 + 布局属性，并负责校验/持久化/导入导出。
 *
 * 变量命名与 JSON 路径的映射见 design/spec/theme-engine.md §2。
 * 兼容层：旧令牌（--bg-base/--border/--text/...）保留为别名，避免 183 条既有规则漏改。
 */

import { PRESETS, DEFAULT_THEME_ID, findPreset, mergeTheme, resolveTheme, BASE } from "./presets.js";

const STORAGE_KEY = "memtool.theme.v1";
const STORAGE_PRESET = "memtool.theme.preset";

/** JSON 路径 → CSS 变量名 */
const VAR_MAP = {
  "tokens.color.bg.base": "--c-bg",
  "tokens.color.bg.raised": "--c-bg-raised",
  "tokens.color.bg.sunken": "--c-bg-sunken",
  "tokens.color.line.base": "--c-line",
  "tokens.color.line.soft": "--c-line-soft",
  "tokens.color.text.primary": "--c-text",
  "tokens.color.text.secondary": "--c-text-dim",
  "tokens.color.text.faint": "--c-text-faint",
  "tokens.color.accent.base": "--c-accent",
  "tokens.color.accent.on": "--c-on-accent",
  "tokens.color.accent.soft": "--c-accent-soft",
  "tokens.color.state.success": "--c-success",
  "tokens.color.state.successSoft": "--c-success-soft",
  "tokens.color.state.warning": "--c-warning",
  "tokens.color.state.warningSoft": "--c-warning-soft",
  "tokens.color.state.danger": "--c-danger",
  "tokens.color.state.dangerSoft": "--c-danger-soft",

  "tokens.typography.fontUi": "--font",
  "tokens.typography.fontBrand": "--font-brand",
  "tokens.typography.fontMono": "--font-mono",
  "tokens.typography.scale.label": "--fs-label",
  "tokens.typography.scale.body": "--fs-body",
  "tokens.typography.scale.title": "--fs-title",
  "tokens.typography.weight.regular": "--fw-regular",
  "tokens.typography.weight.medium": "--fw-medium",
  "tokens.typography.weight.strong": "--fw-strong",
  "tokens.typography.letter.tight": "--ls-tight",
  "tokens.typography.letter.normal": "--ls-normal",
  "tokens.typography.letter.wide": "--ls-wide",
  "tokens.typography.line.tight": "--lh-tight",
  "tokens.typography.line.normal": "--lh-normal",

  "tokens.shape.radius.none": "--r-none",
  "tokens.shape.radius.sm": "--r-sm",
  "tokens.shape.radius.md": "--r-md",
  "tokens.shape.radius.lg": "--r-lg",
  "tokens.shape.border.width": "--bw",
  "tokens.shape.shadow.card": "--sh-card",
  "tokens.shape.shadow.pop": "--sh-pop",

  "tokens.density.space.xs": "--sp-xs",
  "tokens.density.space.sm": "--sp-sm",
  "tokens.density.space.md": "--sp-md",
  "tokens.density.space.lg": "--sp-lg",
  "tokens.density.space.xl": "--sp-xl",
  "tokens.density.control.height": "--ctl-h",
  "tokens.density.control.gap": "--ctl-gap",

  "layout.regions.size.header": "--layout-header-h",
  "layout.regions.size.tabs": "--layout-tabs-h",
  "layout.regions.size.status": "--layout-status-h",
  "layout.main.gap": "--layout-gap",
};

/** 旧令牌别名（保持既有规则可用的过渡层） */
const ALIASES = {
  "--bg-base": "var(--c-bg)",
  "--bg-glass": "var(--c-bg)",
  "--bg-glass-strong": "var(--c-bg-raised)",
  "--bg-input": "var(--c-bg-sunken)",
  "--border": "var(--c-line-soft)",
  "--border-strong": "var(--c-line)",
  "--text": "var(--c-text)",
  "--text-dim": "var(--c-text-dim)",
  "--text-faint": "var(--c-text-faint)",
  "--accent": "var(--c-accent)",
  "--accent-2": "var(--c-accent)",
  "--accent-grad": "var(--c-accent)",
  "--accent-soft": "var(--c-accent-soft)",
  "--green": "var(--c-success)",
  "--green-soft": "var(--c-success-soft)",
  "--red": "var(--c-danger)",
  "--red-soft": "var(--c-danger-soft)",
  "--amber": "var(--c-warning)",
  "--amber-soft": "var(--c-warning-soft)",
  "--radius": "var(--r-lg)",
  "--radius-sm": "var(--r-md)",
  "--shadow": "var(--sh-pop)",
  "--mono": "var(--font-mono)",
};

const HEX_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const COLOR_PATHS = [
  "tokens.color.bg.base", "tokens.color.bg.raised", "tokens.color.bg.sunken",
  "tokens.color.line.base", "tokens.color.line.soft",
  "tokens.color.text.primary", "tokens.color.text.secondary", "tokens.color.text.faint",
  "tokens.color.accent.base", "tokens.color.accent.on", "tokens.color.accent.soft",
  "tokens.color.state.success", "tokens.color.state.successSoft",
  "tokens.color.state.warning", "tokens.color.state.warningSoft",
  "tokens.color.state.danger", "tokens.color.state.dangerSoft",
];

function get(obj, path) {
  return path.split(".").reduce((acc, k) => (acc == null ? undefined : acc[k]), obj);
}

function set(obj, path, value) {
  const keys = path.split(".");
  let cur = obj;
  for (let i = 0; i < keys.length - 1; i += 1) {
    if (typeof cur[keys[i]] !== "object" || cur[keys[i]] === null) cur[keys[i]] = {};
    cur = cur[keys[i]];
  }
  cur[keys[keys.length - 1]] = value;
}

/** 校验并夹紧主题；返回 {theme, warnings}。非法值回退到预设默认，不静默接受。 */
export function normalizeTheme(raw, presetId = DEFAULT_THEME_ID) {
  const warnings = [];
  const preset = findPreset(presetId);
  const base = resolveTheme(preset);
  const theme = mergeTheme(base, raw && typeof raw === "object" ? raw : {});

  for (const path of COLOR_PATHS) {
    const v = get(theme, path);
    if (typeof v !== "string" || !HEX_RE.test(v)) {
      const fallback = get(base, path);
      warnings.push(`${path} 非法色值「${v}」，已回退 ${fallback}`);
      set(theme, path, fallback);
    }
  }

  // 圆角夹紧 0–16px
  const radii = get(theme, "tokens.shape.radius") || {};
  for (const [k, v] of Object.entries(radii)) {
    const n = parseFloat(v);
    if (!Number.isFinite(n) || n < 0 || n > 16) {
      const fb = get(base, `tokens.shape.radius.${k}`);
      warnings.push(`radius.${k}「${v}」越界(0–16px)，已回退 ${fb}`);
      set(theme, `tokens.shape.radius.${k}`, fb);
    } else {
      set(theme, `tokens.shape.radius.${k}`, `${n}px`);
    }
  }

  // 布局：栏宽下限 140px、上限 45%
  const mode = get(theme, "layout.main.mode");
  if (!Array.isArray(mode) || !["split", "stack"].includes(mode[0])) {
    warnings.push("layout.main.mode 非法，已回退 split");
    set(theme, "layout.main.mode", get(base, "layout.main.mode"));
  } else if (mode[0] === "split") {
    const colB = mode[2];
    const px = parseFloat(colB);
    if (Number.isFinite(px) && px < 140) {
      warnings.push(`右栏 ${colB} 小于下限 140px，已夹紧为 140px`);
      theme.layout.main.mode = ["split", mode[1] || "1fr", "140px"];
    }
  }

  const stackBelow = get(theme, "layout.responsive.stackBelow");
  if (!Number.isFinite(stackBelow) || stackBelow < 360) {
    warnings.push(`stackBelow「${stackBelow}」低于 360，已夹紧`);
    set(theme, "layout.responsive.stackBelow", 360);
  }

  return { theme, warnings };
}

/** 把主题写入 documentElement */
export function applyTheme(theme) {
  const root = document.documentElement;
  for (const [path, varName] of Object.entries(VAR_MAP)) {
    const v = get(theme, path);
    if (v !== undefined && v !== null) root.style.setProperty(varName, String(v));
  }
  for (const [alias, target] of Object.entries(ALIASES)) {
    root.style.setProperty(alias, target);
  }

  const layout = theme.layout || BASE.layout;
  const [kind, colA, colB] = layout.main.mode;
  root.dataset.theme = theme.id || DEFAULT_THEME_ID;
  root.dataset.layout = layout.id || "right-rail";
  root.dataset.layoutMode = kind;
  root.style.setProperty("--layout-col-a", kind === "split" ? (colA || "1fr") : "1fr");
  root.style.setProperty("--layout-col-b", kind === "split" ? (colB || "188px") : "0px");
  root.style.setProperty("--layout-stack-below", `${layout.responsive?.stackBelow ?? 460}px`);

  for (const region of ["header", "tabs", "status"]) {
    const on = layout.regions?.visible?.[region] !== false;
    root.style.setProperty(`--layout-${region}-display`, on ? "flex" : "none");
  }

  // 面板次序：input / target 谁在前
  const order = layout.main.order || ["input", "target"];
  root.style.setProperty("--layout-input-order", String(order.indexOf("input")));
  root.style.setProperty("--layout-target-order", String(order.indexOf("target")));
  return theme;
}

/** 读存储 */
export function loadTheme() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const presetId = localStorage.getItem(STORAGE_PRESET) || DEFAULT_THEME_ID;
    if (!raw) return { theme: resolveTheme(findPreset(presetId)), presetId, warnings: [] };
    const parsed = JSON.parse(raw);
    const { theme, warnings } = normalizeTheme(parsed, presetId);
    return { theme, presetId, warnings };
  } catch (e) {
    return { theme: resolveTheme(findPreset(DEFAULT_THEME_ID)), presetId: DEFAULT_THEME_ID, warnings: [`读取主题失败：${e?.message || e}`] };
  }
}

/** 写存储 */
export function saveTheme(theme, presetId) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(theme));
    localStorage.setItem(STORAGE_PRESET, presetId || theme.id || DEFAULT_THEME_ID);
    return true;
  } catch (e) {
    return false;
  }
}

/** 切换到某预设（丢弃用户覆盖层） */
export function applyPreset(id) {
  const preset = findPreset(id);
  const theme = resolveTheme(preset);
  theme.id = preset.id;
  theme.name = preset.name;
  applyTheme(theme);
  saveTheme(theme, preset.id);
  return theme;
}

/** 从任意 JSON 导入 */
export function importTheme(json) {
  const parsed = typeof json === "string" ? JSON.parse(json) : json;
  const presetId = parsed?.id && PRESETS.some((p) => p.id === parsed.id)
    ? parsed.id
    : DEFAULT_THEME_ID;
  const { theme, warnings } = normalizeTheme(parsed, presetId);
  theme.id = parsed?.id || `custom-${Date.now().toString(36)}`;
  theme.name = parsed?.name || "自定义";
  applyTheme(theme);
  saveTheme(theme, presetId);
  return { theme, warnings };
}

/** 导出（含完整令牌与布局，可原样导回） */
export function exportTheme(theme) {
  return JSON.stringify(
    { id: theme.id, name: theme.name, meta: theme.meta, tokens: theme.tokens, layout: theme.layout },
    null,
    2,
  );
}

export { PRESETS, DEFAULT_THEME_ID, findPreset, resolveTheme, mergeTheme };
