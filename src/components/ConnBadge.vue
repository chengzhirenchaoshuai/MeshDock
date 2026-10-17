<script setup lang="ts">
import { computed } from "vue";
import type { Conn } from "../api";
import { connTone } from "../format";

const props = defineProps<{ conn: Conn }>();

// 状态色取自 style.css 的 --md-* 变量，背景用同色的淡化版
const color = computed(() => `var(--md-${connTone(props.conn.kind)})`);
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium whitespace-nowrap"
    :style="{ color, background: `color-mix(in srgb, ${color} 13%, transparent)` }"
  >
    <span
      class="size-1.5 rounded-full"
      :class="{ 'animate-pulse': conn.kind === 'punching' }"
      :style="{ background: color }"
    />
    {{ conn.label }}
  </span>
</template>
