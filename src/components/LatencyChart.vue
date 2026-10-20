<script lang="ts">
/** 一天的毫秒数；放在普通 script 里，defineProps 的默认值才能引用 */
const DAY = 24 * 3600 * 1000;
</script>

<script setup lang="ts">
import { computed, ref } from "vue";
import type { ConnKind, Sample } from "../api";
import { clock, formatLatency, kindLabel, latencyStats, timeTicks } from "../format";

/**
 * 延迟图：直连实线、中继虚线，没有回应画成底部灰色短竖线。
 * 设备卡片上是 24 小时小图；详情弹窗里用更长的时间跨度、更高的图，并显示时间刻度
 */
const props = withDefaults(
  defineProps<{
    samples: Sample[];
    now: number;
    /** 时间跨度（毫秒），默认 24 小时 */
    span?: number;
    /** 图的高度（像素），默认 40 */
    height?: number;
    /** 显示时间刻度（详情弹窗用） */
    axis?: boolean;
  }>(),
  { span: DAY, height: 40, axis: false },
);

const W = 300;
const PAD = 3; // 上下留白，线宽 2px 不被裁掉
/** 相邻两次测量间隔超过 15 分钟就断开，不连线（程序没运行或设备离线） */
const GAP = 15 * 60 * 1000;

const H = computed(() => props.height);
const isRelay = (k: ConnKind | null) => k === "relay" || k === "peer-relay";

const start = computed(() => props.now - props.span);
/** 只画时间跨度内的测量 */
const inSpan = computed(() => props.samples.filter((s) => s.atMs >= start.value));
const ok = computed(() => inSpan.value.filter((s) => s.ms != null));
/** 纵轴上限：至少 5 ms，避免局域网 1 ms 的数据被放大成满屏抖动 */
const maxMs = computed(() => Math.max(5, ...ok.value.map((s) => s.ms!)));

const x = (at: number) => ((at - start.value) / props.span) * W;
const y = (ms: number) => H.value - PAD - (ms / maxMs.value) * (H.value - 2 * PAD);

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
const timeouts = computed(() => inSpan.value.filter((s) => s.ms == null));

/** 时间跨度的中文：“24 小时” / “7 天” */
const spanLabel = computed(() => (props.span % DAY === 0 && props.span > DAY ? `${props.span / DAY} 天` : `${Math.round(props.span / 3_600_000)} 小时`));

/** 摘要文字：中位延迟、中继占比、无回应次数 */
const summary = computed(() => {
  const st = latencyStats(inSpan.value);
  if (!st.count) return "";
  const parts = [`中位 ${st.median != null ? formatLatency(st.median) : "无"}`];
  if (st.relayRatio != null) parts.push(`中继 ${Math.round(st.relayRatio * 100)}%`);
  if (st.timeouts) parts.push(`无回应 ${st.timeouts} 次`);
  return `${spanLabel.value} ${st.count} 次测量：${parts.join(" · ")}`;
});

/** 跨度超过一天时，时间要带日期 */
const when = (ms: number) => {
  if (props.span <= DAY) return clock(ms);
  const d = new Date(ms);
  return `${d.getMonth() + 1}/${d.getDate()} ${clock(ms).slice(0, 5)}`;
};

/** 时间刻度：24 小时每 6 小时一格，更长时每天一格 */
const ticks = computed(() => (props.axis ? timeTicks(start.value, props.now) : []));

/** 刻度文字居中对准刻度线；贴近两端时改为靠边对齐，免得被裁掉 */
function tickStyle(at: number) {
  const pct = (x(at) / W) * 100;
  const shift = pct < 4 ? "0" : pct > 96 ? "-100%" : "-50%";
  return { left: `${pct}%`, transform: `translateX(${shift})` };
}

// ---------- 悬停：竖线 + 提示 ----------
const hover = ref<Sample | null>(null);
function onMove(e: MouseEvent) {
  const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
  const at = start.value + ((e.clientX - rect.left) / rect.width) * props.span;
  let best: Sample | null = null;
  for (const s of inSpan.value) {
    if (!best || Math.abs(s.atMs - at) < Math.abs(best.atMs - at)) best = s;
  }
  // 离最近的测量太远（跨度的 0.7%，24 小时约 10 分钟）就不显示，避免指向空白处时给出误导
  hover.value = best && Math.abs(best.atMs - at) < props.span * 0.007 ? best : null;
}
const hoverText = computed(() => {
  const s = hover.value;
  if (!s) return "";
  if (s.ms == null) return `${when(s.atMs)} · 没有回应`;
  const kind = s.kind === "peer-relay" ? "节点中转" : kindLabel(s.kind) || "路径未知";
  return `${when(s.atMs)} · ${kind} · ${formatLatency(s.ms)}`;
});
</script>

<template>
  <div class="flex flex-col gap-1">
    <div class="relative">
      <svg
        :viewBox="`0 0 ${W} ${H}`"
        preserveAspectRatio="none"
        class="block w-full overflow-visible"
        :style="{ height: `${H}px` }"
        role="img"
        :aria-label="summary"
        @mousemove="onMove"
        @mouseleave="hover = null"
      >
        <!-- 时间刻度的竖向参考线 -->
        <line
          v-for="t in ticks"
          :key="t.at"
          :x1="x(t.at)"
          :x2="x(t.at)"
          y1="0"
          :y2="H"
          stroke="var(--md-line)"
          stroke-width="1"
          stroke-dasharray="2 3"
          vector-effect="non-scaling-stroke"
        />
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
        <!-- 孤立点用短竖线代替圆点：图被横向拉伸时圆点会变成椭圆 -->
        <line
          v-for="s in dots"
          :key="s.atMs"
          :x1="x(s.atMs)"
          :x2="x(s.atMs)"
          :y1="y(s.ms!) - 2"
          :y2="y(s.ms!) + 2"
          stroke-width="3"
          stroke-linecap="round"
          vector-effect="non-scaling-stroke"
          :stroke="isRelay(s.kind) ? 'var(--md-chart-relay)' : 'var(--md-chart-direct)'"
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
      <!-- 纵轴上限（详情图） -->
      <span v-if="axis" class="num pointer-events-none absolute top-0 left-1 text-[10px] text-faint">{{ formatLatency(maxMs) }}</span>
      <div
        v-if="hover"
        class="num pointer-events-none absolute -top-7 z-10 rounded-ctl border border-line bg-surface px-2 py-0.5 text-[11px] whitespace-nowrap text-fg shadow-card"
        :style="{ left: `${Math.min(Math.max((x(hover.atMs) / W) * 100, 15), 85)}%`, transform: 'translateX(-50%)' }"
      >
        {{ hoverText }}
      </div>
    </div>
    <!-- 时间刻度文字 -->
    <div v-if="axis" class="num relative h-4 text-[10px] text-faint">
      <span
        v-for="t in ticks"
        :key="t.at"
        class="absolute whitespace-nowrap"
        :style="tickStyle(t.at)"
      >
        {{ t.label }}
      </span>
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
