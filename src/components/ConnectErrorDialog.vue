<script setup lang="ts">
import { LoaderCircle, TriangleAlert } from "@lucide/vue";
import type { CliError } from "../api";

/** 连接组网失败：显示原因，可展开原始输出并重试 */
defineProps<{ error: CliError; busy: boolean }>();
const emit = defineEmits<{ retry: []; close: [] }>();
</script>

<template>
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/30 p-6" @click.self="emit('close')">
    <div class="flex max-h-full w-full max-w-lg flex-col gap-3 rounded-card border border-line bg-surface p-5 shadow-card">
      <h2 class="flex items-center gap-2 text-base font-semibold">
        <TriangleAlert class="size-5" style="color: var(--md-error)" />
        连接失败
      </h2>
      <p class="text-sm">{{ error.message }}</p>
      <details v-if="error.raw" class="min-h-0 text-sm">
        <summary class="cursor-pointer text-muted select-none">查看原始输出</summary>
        <pre class="num mt-2 max-h-72 overflow-auto rounded-ctl bg-subtle p-2 text-xs whitespace-pre-wrap select-text">{{ error.raw }}</pre>
      </details>
      <div class="flex justify-end gap-2">
        <button class="rounded-ctl border border-line px-4 py-1.5 text-sm hover:bg-subtle" @click="emit('close')">
          关闭
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-ctl bg-accent px-4 py-1.5 text-sm text-accent-fg hover:bg-accent-hover disabled:opacity-60"
          :disabled="busy"
          @click="emit('retry')"
        >
          <LoaderCircle v-if="busy" class="size-4 animate-spin" />
          重试
        </button>
      </div>
    </div>
  </div>
</template>
