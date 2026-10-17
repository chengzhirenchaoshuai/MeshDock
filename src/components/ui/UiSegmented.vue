<script setup lang="ts" generic="T extends string | number">
import type { Component } from "vue";

/** 分段选择：options 列出可选值，v-model 绑定当前值；mono 让文字用等宽字体 */
defineProps<{ options: { value: T; label: string; icon?: Component }[]; mono?: boolean }>();
const model = defineModel<T>({ required: true });
</script>

<template>
  <div class="inline-flex w-fit gap-1 rounded-ctl border border-line bg-subtle p-1" role="radiogroup">
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :aria-checked="model === o.value"
      class="inline-flex items-center gap-1.5 rounded-[7px] px-3 py-1 text-sm transition-all duration-150"
      :class="[
        model === o.value ? 'bg-surface font-medium text-fg shadow-card' : 'text-muted hover:text-fg',
        { num: mono },
      ]"
      @click="model = o.value"
    >
      <component :is="o.icon" v-if="o.icon" class="size-4" :class="{ 'text-accent': model === o.value }" />
      {{ o.label }}
    </button>
  </div>
</template>
