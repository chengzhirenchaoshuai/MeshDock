<script setup lang="ts">
import { computed, ref } from "vue";
import type { ConnKind, Sample } from "../api";
import { clock, formatLatency } from "../format";

/** 最近 24 小时的延迟小图：直连实线、中继虚线，没有回应画成底部灰色短竖线 */
const props = defineProps<{ samples: Sample[]; now: number }>();

const W = 300;
const H = 40;
const PAD = 3; // 上下留白，线宽 2px 不被裁掉
const SPAN = 24 * 3600 * 1000;
/** 相邻两次测量间隔超过 15 分钟就断开，不连线（程序没运行或设备离线） */
const GAP = 15 * 60 * 1000;

const isRelay = (k: ConnKind | null) => k === "relay" || k === "peer-relay";
const kindLabel: Partial<Record<ConnKind, string>> = {
  lan: "局域网直连",
  ipv6: "IPv6 直连",
  public: "公网直连",
  relay: "中继",
  "peer-relay": "节点中转",
};

const start = computed(() => props.now - SPAN);
const ok = computed(() => props.samples.filter((s) => s.ms != null));
/** 纵轴上限：至少 5 ms，避免局域网 1 ms 的数据被放大成满屏抖动 */
const maxMs = computed(() => Math.max(5, ...ok.value.map((s) => s.ms!)));

const x = (at: number) => ((at - start.value) / SPAN) * W;
const y = (ms: number) => H - PAD - (ms / maxMs.value) * (H - 2 * PAD);

/** 相邻两个有回应的点连成一段，按后一个点的路径决定实线 / 虚线和颜色 */
const segments = computed(() => {
  const list = ok.value;
  const out: { d: string; relay: boolean }[] = [];
  for (let i = 1; i < list.length; i++) {
    const a = list[i - 1];
    const b = list[i];
    if (b.atMs - a.atMs > GAP) continue;
    out.push({
      d: `M${x(a.atMs).toFixed(1)},${y(a.ms!).toFixed(1)}L${x(b.atMs).toFixed(1)},${y(b.ms!).toFixed(1)}`,
      relay: isRelay(b.kind),
    });
  }
  return out;
});

/** 孤立的点（前后都断开）画成小圆点，否则看不到 */
const dots = computed(() =>
  ok.value.filter((s, i, list) => {
    const prev = list[i - 1];
    const next = list[i + 1];
    return (!prev || s.atMs - prev.atMs > GAP) && (!next || next.atMs - s.atMs > GAP);
  }),
);
const timeouts = computed(() => props.samples.filter((s) => s.ms == null));

/** 摘要文字：中位延迟、中继占比、无回应次数 */
const summary = computed(() => {
  const n = props.samples.length;
  if (!n) return "";
  const sorted = ok.value.map((s) => s.ms!).sort((a, b) => a - b);
  const median = sorted.length ? formatLatency(sorted[Math.floor(sorted.length / 2)]) : "无";
  const relay = ok.value.filter((s) => isRelay(s.kind)).length;
  const parts = [`中位 ${median}`];
  if (ok.value.length) parts.push(`中继 ${Math.round((relay / ok.value.length) * 100)}%`);
  if (timeouts.value.length) parts.push(`无回应 ${timeouts.value.length} 次`);
  return `24 小时 ${n} 次测量：${parts.join(" · ")}`;
});

// ---------- 悬停：竖线 + 提示 ----------
const hover = ref<Sample | null>(null);
function onMove(e: MouseEvent) {
  const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
  const at = start.value + ((e.clientX - rect.left) / rect.width) * SPAN;
  let best: Sample | null = null;
  for (const s of props.samples) {
    if (!best || Math.abs(s.atMs - at) < Math.abs(best.atMs - at)) best = s;
  }
  // 离最近的测量超过 10 分钟就不显示，避免指向空白处时给出误导
  hover.value = best && Math.abs(best.atMs - at) < 10 * 60 * 1000 ? best : null;
}
const hoverText = computed(() => {
  const s = hover.value;
  if (!s) return "";
  if (s.ms == null) return `${clock(s.atMs)} · 没有回应`;
  return `${clock(s.atMs)} · ${s.kind ? (kindLabel[s.kind] ?? "路径未知") : "路径未知"} · ${formatLatency(s.ms)}`;
});
</script>

<template>
  <div class="flex flex-col gap-1">
    <div class="relative">
      <svg
        :viewBox="`0 0 ${W} ${H}`"
        preserveAspectRatio="none"
        class="block h-10 w-full overflow-visible"
        role="img"
        :aria-label="summary"
        @mousemove="onMove"
        @mouseleave="hover = null"
      >
        <!-- 基线 -->
        <line :x1="0" :x2="W" :y1="H - 0.5" :y2="H - 0.5" stroke="var(--md-line)" stroke-width="1" vector-effect="non-scaling-stroke" />
        <path
          v-for="(s, i) in segments"
          :key="i"
          :d="s.d"
          fill="none"
          stroke-width="2"
          stroke-linecap="round"
          vector-effect="non-scaling-stroke"
          :stroke="s.relay ? 'var(--md-chart-relay)' : 'var(--md-chart-direct)'"
          :stroke-dasharray="s.relay ? '4 3' : undefined"
        />
        <circle
          v-for="s in dots"
          :key="s.atMs"
          :cx="x(s.atMs)"
          :cy="y(s.ms!)"
          r="2"
          :fill="isRelay(s.kind) ? 'var(--md-chart-relay)' : 'var(--md-chart-direct)'"
        />
        <rect v-for="s in timeouts" :key="`t${s.atMs}`" :x="x(s.atMs) - 1" :y="H - 6" width="2" height="6" fill="var(--md-offline)" />
        <!-- 悬停竖线 -->
        <line
          v-if="hover"
          :x1="x(hover.atMs)"
          :x2="x(hover.atMs)"
          y1="0"
          :y2="H"
          stroke="var(--md-muted)"
          stroke-width="1"
          vector-effect="non-scaling-stroke"
        />
      </svg>
      <div
        v-if="hover"
        class="num pointer-events-none absolute -top-7 z-10 rounded-ctl border border-line bg-surface px-2 py-0.5 text-[11px] whitespace-nowrap text-fg shadow-card"
        :style="{ left: `${Math.min(Math.max((x(hover.atMs) / W) * 100, 15), 85)}%`, transform: 'translateX(-50%)' }"
      >
        {{ hoverText }}
      </div>
    </div>
    <div class="flex flex-wrap items-center gap-x-3 text-[11px] text-muted">
      <span>{{ summary }}</span>
      <span class="ml-auto inline-flex items-center gap-1">
        <svg width="14" height="4"><line x1="0" x2="14" y1="2" y2="2" stroke="var(--md-chart-direct)" stroke-width="2" /></svg>直连
      </span>
      <span class="inline-flex items-center gap-1">
        <svg width="14" height="4"><line x1="0" x2="14" y1="2" y2="2" stroke="var(--md-chart-relay)" stroke-width="2" stroke-dasharray="4 3" /></svg>中继
      </span>
    </div>
  </div>
</template>
