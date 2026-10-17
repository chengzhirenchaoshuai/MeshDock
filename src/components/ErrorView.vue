<script setup lang="ts">
import { RotateCw, TriangleAlert } from "@lucide/vue";
import type { CliError } from "../api";
import UiButton from "./ui/UiButton.vue";

defineProps<{ error: CliError; busy: boolean }>();
const emit = defineEmits<{ retry: [] }>();
</script>

<template>
  <section class="mx-auto flex max-w-md flex-col items-center gap-4 py-16 text-center">
    <div class="relative grid size-14 place-items-center rounded-full text-error" style="background: var(--md-error-soft)">
      <span class="absolute inset-0 rounded-full border" style="border-color: color-mix(in srgb, var(--md-error) 30%, transparent)" />
      <TriangleAlert class="size-7" />
    </div>
    <p class="text-base font-medium">{{ error.message }}</p>
    <UiButton variant="primary" :icon="RotateCw" :loading="busy" :disabled="busy" @click="emit('retry')">
      {{ busy ? "正在重试…" : "重试" }}
    </UiButton>
    <details v-if="error.raw" class="w-full text-left text-xs text-muted">
      <summary class="cursor-pointer select-none hover:text-fg">查看原始信息</summary>
      <pre class="ui-code num mt-2 overflow-auto p-3 whitespace-pre-wrap select-text">{{ error.raw }}</pre>
    </details>
  </section>
</template>
