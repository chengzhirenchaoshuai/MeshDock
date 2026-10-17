<script setup lang="ts">
import { computed } from "vue";
import { CircleHelp, Laptop, Monitor, Server, Smartphone } from "@lucide/vue";

const props = defineProps<{ os: string | null }>();

// 按系统选图标；未知系统显示问号
const icon = computed(() => {
  switch ((props.os ?? "").toLowerCase()) {
    case "windows":
      return Monitor;
    case "linux":
      return Server;
    case "android":
    case "ios":
      return Smartphone;
    case "macos":
      return Laptop;
    default:
      return CircleHelp;
  }
});
</script>

<template>
  <component :is="icon" class="size-5" :title="os ?? '未知系统'" />
</template>
