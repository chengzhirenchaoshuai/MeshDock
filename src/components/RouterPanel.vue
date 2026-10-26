<script setup lang="ts">
import { computed, type Component } from "vue";
import { CircleAlert, CircleCheck, CircleX, Info, Router } from "@lucide/vue";
import type { Level, RouterHealth } from "../api";
import { formatLatency, timeAgo } from "../format";
import StatusPill from "./ui/StatusPill.vue";
import UiCard from "./ui/UiCard.vue";

const props = defineProps<{ router: RouterHealth; now: number }>();

/** 结论级别 → 图标、状态色名称、文字（与网络体检一致） */
const levelStyle: Record<Level, { icon: Component; tone: string; label: string }> = {
  good: { icon: CircleCheck, tone: "direct", label: "正常" },
  info: { icon: Info, tone: "idle", label: "正常" },
  warn: { icon: CircleAlert, tone: "punching", label: "注意" },
  bad: { icon: CircleX, tone: "error", label: "有问题" },
};
/** 右上角总体徽标：最严重的只是“说明”时也算正常，用绿色对勾，避免灰色的“正常”看起来像不可用 */
const overall = computed(() => levelStyle[props.router.level === "info" ? "good" : props.router.level]);

/** 设备信息：延迟、版本、型号、系统，每项一个小标签 */
const meta = computed(() => {
  const r = props.router;
  return [
    r.relayMs != null ? `到${r.relayName}中继 ${formatLatency(r.relayMs)}` : null,
    r.version ? `Tailscale ${r.version}` : null,
    r.deviceModel,
    r.osVersion ? `系统 ${r.osVersion}` : null,
    !r.online && r.lastSeenMs ? `最后在线 ${timeAgo(r.lastSeenMs, props.now)}` : null,
  ].filter(Boolean);
});
</script>

<template>
  <UiCard :title="`子网路由器 · ${router.name}`" :icon="Router">
    <template #actions>
      <span class="num text-xs text-muted">{{ router.routes.join("、") }}</span>
      <StatusPill class="ml-auto" :tone="overall.tone" :label="overall.label" :icon="overall.icon" />
    </template>
    <p class="mb-3 text-xs text-muted">出门模式下访问家里局域网全靠它，这里是它自己上报的网络情况。</p>
    <ul class="flex flex-col gap-2 text-sm">
      <li v-for="(f, i) in router.findings" :key="i" class="flex items-start gap-2.5 leading-relaxed">
        <span
          class="mt-px grid size-5 shrink-0 place-items-center rounded-full"
          :style="{ color: `var(--md-${levelStyle[f.level].tone})`, background: `var(--md-${levelStyle[f.level].tone}-soft)` }"
        >
          <component :is="levelStyle[f.level].icon" class="size-3.5" />
        </span>
        <span>{{ f.text }}</span>
      </li>
    </ul>
    <div v-if="meta.length" class="mt-3 flex flex-wrap gap-1.5 border-t border-line pt-3">
      <span v-for="m in meta" :key="m!" class="rounded-full border border-line bg-subtle px-2 py-0.5 text-[11px] text-muted">
        {{ m }}
      </span>
    </div>
  </UiCard>
</template>
