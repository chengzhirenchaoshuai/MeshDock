<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import type { CliError } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiDialog from "./ui/UiDialog.vue";

/** 连接组网失败：显示原因，可展开原始输出并重试 */
defineProps<{ error: CliError; busy: boolean }>();
const emit = defineEmits<{ retry: []; close: [] }>();
</script>

<template>
  <UiDialog title="连接失败" :icon="TriangleAlert" tone="error" width="lg" @close="emit('close')">
    <p class="text-sm leading-relaxed">{{ error.message }}</p>
    <details v-if="error.raw" class="min-h-0 text-sm">
      <summary class="cursor-pointer text-muted select-none hover:text-fg">查看原始输出</summary>
      <pre class="ui-code num mt-2 max-h-72 overflow-auto p-3 text-xs whitespace-pre-wrap select-text">{{ error.raw }}</pre>
    </details>
    <template #footer>
      <UiButton @click="emit('close')">关闭</UiButton>
      <UiButton variant="primary" :loading="busy" :disabled="busy" @click="emit('retry')">重试</UiButton>
    </template>
  </UiDialog>
</template>
