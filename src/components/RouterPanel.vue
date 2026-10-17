<script setup lang="ts">
import { computed } from "vue";
import { CircleAlert, CircleCheck, CircleX, Info, Router } from "@lucide/vue";
import type { Level, RouterHealth } from "../api";
import { formatLatency, timeAgo } from "../format";

const props = defineProps<{ router: RouterHealth; now: number }>();

/** 结论级别 → 图标、状态色、文字（与网络体检一致） */
const levelStyle: Record<Level, { icon: unknown; color: string; label: string }> = {
  good: { icon: CircleCheck, color: "var(--md-direct)", label: "正常" },
  info: { icon: Info, color: "var(--md-idle)", label: "正常" },
  warn: { icon: CircleAlert, color: "var(--md-punching)", label: "注意" },
  bad: { icon: CircleX, color: "var(--md-error)", label: "有问题" },
};
const overall = computed(() => levelStyle[props.router.level]);

/** 设备信息一行：延迟、版本、型号、系统 */
const meta = computed(() => {
  const r = props.router;
  return [
    r.relayMs != null ? `到广州中继 ${formatLatency(r.relayMs)}` : null,
    r.version ? `Tailscale ${r.version}` : null,
    r.deviceModel,
    r.osVersion ? `系统 ${r.osVersion}` : null,
    !r.online && r.lastSeenMs ? `最后在线 ${timeAgo(r.lastSeenMs, props.now)}` : null,
  ].filter(Boolean);
});
</script>

<template>
  <section class="flex flex-col gap-3 rounded-card border border-line bg-surface p-4 shadow-card">
    <header class="flex items-center gap-2">
      <Router class="size-4 text-muted" />
      <h2 class="text-sm font-semibold">子网路由器 · {{ router.name }}</h2>
      <span class="num text-xs text-muted">{{ router.routes.join("、") }}</span>
      <span
        class="ml-auto inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium"
        :style="{ color: overall.color, background: `color-mix(in srgb, ${overall.color} 13%, transparent)` }"
      >
        <component :is="overall.icon" class="size-3.5" />
        {{ overall.label }}
      </span>
    </header>
    <p class="text-xs text-muted">出门模式下访问家里局域网全靠它，这里是它自己上报的网络情况。</p>
    <ul class="flex flex-col gap-1.5 text-sm">
      <li v-for="(f, i) in router.findings" :key="i" class="flex items-start gap-2 leading-relaxed">
        <component
          :is="levelStyle[f.level].icon"
          class="mt-0.5 size-4 shrink-0"
          :style="{ color: levelStyle[f.level].color }"
        />
        <span>{{ f.text }}</span>
      </li>
    </ul>
    <p v-if="meta.length" class="border-t border-line pt-2 text-xs text-muted">{{ meta.join(" · ") }}</p>
  </section>
</template>
