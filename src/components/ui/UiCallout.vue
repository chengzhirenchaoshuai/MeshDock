<script setup lang="ts">
import type { Component } from "vue";
import { TriangleAlert } from "@lucide/vue";

/** 提示条：tone 决定颜色，默认是警告图标；action 插槽放右侧按钮 */
withDefaults(defineProps<{ tone: string; icon?: Component }>(), { icon: () => TriangleAlert });
</script>

<template>
  <div
    class="flex items-center gap-3 rounded-ctl border border-l-[3px] px-3 py-2.5 text-sm leading-relaxed"
    :style="{
      background: `var(--md-${tone}-soft)`,
      borderColor: `color-mix(in srgb, var(--md-${tone}) 30%, transparent)`,
      borderLeftColor: `var(--md-${tone})`,
    }"
  >
    <component :is="icon" class="size-5 shrink-0 self-start" :style="{ color: `var(--md-${tone})` }" />
    <div class="min-w-0 flex-1"><slot /></div>
    <slot name="action" />
  </div>
</template>
