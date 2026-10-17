<script setup lang="ts">
import type { Component } from "vue";
import { LoaderCircle } from "@lucide/vue";

/**
 * 统一按钮：variant 选用途，size 选大小；loading 时图标换成转圈
 * disabled、title、点击事件直接写在组件上，会落到内部的 button
 */
withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger" | "danger-outline";
    size?: "xs" | "sm" | "md";
    loading?: boolean;
    icon?: Component;
  }>(),
  { variant: "secondary", size: "md", loading: false, icon: undefined },
);
</script>

<template>
  <button type="button" class="ui-btn" :class="[`ui-btn-${variant}`, `ui-btn-${size}`]">
    <LoaderCircle v-if="loading" class="ui-btn-icon animate-spin" />
    <component :is="icon" v-else-if="icon" class="ui-btn-icon" />
    <slot />
  </button>
</template>
