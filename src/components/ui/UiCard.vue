<script setup lang="ts">
import type { Component } from "vue";

/**
 * 卡片外壳：可选标题和图标，actions 插槽放在标题行右侧
 * interactive：鼠标悬停时上浮、阴影加深
 */
withDefaults(
  defineProps<{ as?: string; title?: string; icon?: Component; interactive?: boolean }>(),
  { as: "section", title: undefined, icon: undefined, interactive: false },
);
</script>

<template>
  <component :is="as" class="ui-card p-4" :class="{ 'ui-card-interactive': interactive }">
    <header v-if="title || $slots.actions" class="mb-3 flex items-center gap-2.5">
      <span
        v-if="icon"
        class="grid size-7 shrink-0 place-items-center rounded-ctl text-accent"
        style="background: var(--md-accent-soft)"
      >
        <component :is="icon" class="size-4" />
      </span>
      <h2 v-if="title" class="text-sm font-semibold">{{ title }}</h2>
      <slot name="actions" />
    </header>
    <slot />
  </component>
</template>
