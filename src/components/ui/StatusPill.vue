<script setup lang="ts">
import type { Component } from "vue";

/**
 * 状态胶囊：tone 是状态色名称（direct、relay、offline、punching、error、idle、accent）
 * 有 icon 时显示图标，否则显示小圆点；pulse 让圆点闪烁（只给“打洞中”用）
 */
defineProps<{ tone: string; label: string; icon?: Component; pulse?: boolean }>();
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium whitespace-nowrap transition-colors duration-300"
    :style="{
      color: `var(--md-${tone})`,
      background: `var(--md-${tone}-soft)`,
      borderColor: `color-mix(in srgb, var(--md-${tone}) 25%, transparent)`,
    }"
  >
    <component :is="icon" v-if="icon" class="size-3.5" />
    <span v-else class="relative flex size-1.5">
      <span
        v-if="pulse"
        class="absolute inline-flex size-full animate-ping rounded-full opacity-75"
        :style="{ background: `var(--md-${tone})` }"
      />
      <span class="relative inline-flex size-1.5 rounded-full" :style="{ background: `var(--md-${tone})` }" />
    </span>
    {{ label }}
  </span>
</template>
