<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import type { CliError } from "../api";

defineProps<{ error: CliError; busy: boolean }>();
const emit = defineEmits<{ retry: [] }>();
</script>

<template>
  <section class="mx-auto flex max-w-md flex-col items-center gap-4 py-16 text-center">
    <div class="grid size-12 place-items-center rounded-full bg-subtle text-error">
      <TriangleAlert class="size-6" />
    </div>
    <p class="text-base font-medium">{{ error.message }}</p>
    <button
      class="rounded-ctl bg-accent px-4 py-1.5 text-accent-fg hover:bg-accent-hover disabled:opacity-60"
      :disabled="busy"
      @click="emit('retry')"
    >
      {{ busy ? "正在重试…" : "重试" }}
    </button>
    <details v-if="error.raw" class="w-full text-left text-xs text-muted">
      <summary class="cursor-pointer select-none">查看原始信息</summary>
      <pre class="num mt-2 overflow-auto rounded-ctl bg-subtle p-3 whitespace-pre-wrap select-text">{{ error.raw }}</pre>
    </details>
  </section>
</template>
