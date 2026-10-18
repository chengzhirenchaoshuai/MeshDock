/** 配色方案：与后端 settings.rs 的 PALETTE_CHOICES、style.css 的 data-palette 一致 */
export const PALETTES = [
  {
    id: "aurora",
    label: "极光",
    desc: "紫到蓝，默认",
    // 设置页小样用的颜色：渐变两端、深色背景、深色卡片
    swatch: { from: "#7048e8", to: "#2f6fe0", bg: "#0a0e1a", card: "#121829" },
  },
  {
    id: "ocean",
    label: "深海",
    desc: "蓝到青，沉稳",
    swatch: { from: "#2563eb", to: "#0e7490", bg: "#07111d", card: "#0e1a2a" },
  },
  {
    id: "neon",
    label: "霓虹",
    desc: "品红到紫，鲜艳",
    swatch: { from: "#c026d3", to: "#7c3aed", bg: "#0f0a1a", card: "#181127" },
  },
  {
    id: "jade",
    label: "翡翠",
    desc: "绿到青绿，清新",
    swatch: { from: "#047857", to: "#0f766e", bg: "#06120e", card: "#0d1c17" },
  },
  {
    id: "graphite",
    label: "石墨",
    desc: "中性灰，不带彩色",
    swatch: { from: "#f4f4f5", to: "#a1a1aa", bg: "#0b0b0d", card: "#151517" },
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
