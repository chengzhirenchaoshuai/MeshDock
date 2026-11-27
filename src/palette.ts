/** 配色方案：与后端 settings.rs 的 PALETTE_CHOICES、style.css 的 data-palette 一致 */
export const PALETTES = [
  {
    id: "aurora",
    label: "极光",
    desc: "紫到蓝，默认",
    // 设置页小样用的颜色：渐变两端、深色背景、深色卡片；浅色顶部极光、浅色背景、浅色主按钮
    // 浅色三个值与 style.css 里该方案的 --md-hero-top、--md-bg、--md-accent 一致
    swatch: { from: "#7048e8", to: "#2f6fe0", bg: "#0a0e1a", card: "#121829", lightTop: "#ede9ff", lightBg: "#f9f9fb", lightAccent: "#6a46e5" },
  },
  {
    id: "ocean",
    label: "深海",
    desc: "蓝到青，沉稳",
    swatch: { from: "#2563eb", to: "#0e7490", bg: "#07111d", card: "#0e1a2a", lightTop: "#e4eefd", lightBg: "#f9f9fb", lightAccent: "#1d5fd0" },
  },
  {
    id: "neon",
    label: "樱花",
    desc: "淡樱粉，柔和",
    swatch: { from: "#fbc4dc", to: "#f48fbd", bg: "#140b10", card: "#1e1218", lightTop: "#fde6ef", lightBg: "#fdf7f9", lightAccent: "#f7a8cb" },
  },
  {
    id: "jade",
    label: "青草",
    desc: "浅草绿，清新",
    swatch: { from: "#a3d65c", to: "#6ab535", bg: "#0b1209", card: "#121c10", lightTop: "#e4f3d0", lightBg: "#f5f9ef", lightAccent: "#8fd14f" },
  },
] as const;

export type PaletteId = (typeof PALETTES)[number]["id"];

/** 本地缓存的键：启动时先按上次的方案着色，避免设置读回来之前闪一下默认配色 */
const CACHE_KEY = "meshdock.palette";

const isPalette = (v: unknown): v is PaletteId => PALETTES.some((p) => p.id === v);

/** 把方案写到 <html data-palette>，并记到本地缓存；不认识的值按默认方案处理 */
export function applyPalette(id: string | null | undefined) {
  const palette = isPalette(id) ? id : PALETTES[0].id;
  document.documentElement.dataset.palette = palette;
  try {
    localStorage.setItem(CACHE_KEY, palette);
  } catch {
    // 本地存储不可用时只是启动时可能闪一下，不影响使用
  }
}

/** 上次使用的方案（启动时调用） */
export function cachedPalette(): string | null {
  try {
    return localStorage.getItem(CACHE_KEY);
  } catch {
    return null;
  }
}
