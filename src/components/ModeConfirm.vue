<script setup lang="ts">
import { LoaderCircle, TriangleAlert } from "@lucide/vue";
import type { CliError } from "../api";

/** on：要切到的模式（true 出门、false 在家） */
defineProps<{ on: boolean; atHome: boolean | null; busy: boolean; error: CliError | null }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/30 p-6" @click.self="!busy && emit('cancel')">
    <div class="flex w-full max-w-md flex-col gap-4 rounded-card border border-line bg-surface p-5 shadow-card">
      <h2 class="text-base font-semibold">切换到{{ on ? "出门模式" : "在家模式" }}？</h2>
      <p class="text-sm leading-relaxed text-muted">
        将执行 <code class="num rounded bg-subtle px-1">tailscale set --accept-routes={{ on }}</code>。
        {{ on ? "开启后可以通过子网路由访问家里局域网的设备。" : "关闭后访问家里局域网不再经过隧道。" }}
      </p>
      <p
        v-if="on && atHome === true"
        class="flex items-start gap-2 rounded-ctl px-3 py-2 text-sm"
        style="color: var(--md-error); background: color-mix(in srgb, var(--md-error) 12%, transparent)"
      >
        <TriangleAlert class="mt-0.5 size-4 shrink-0" />
        你现在在家。在家开启出门模式会让本地流量绕进隧道，可能断网，确定吗？
      </p>
      <details v-if="error" class="text-sm" open>
        <summary class="cursor-pointer select-none" style="color: var(--md-error)">{{ error.message }}</summary>
        <pre class="num mt-2 overflow-auto rounded-ctl bg-subtle p-2 text-xs whitespace-pre-wrap select-text">{{ error.raw }}</pre>
      </details>
      <div class="flex justify-end gap-2">
        <button class="rounded-ctl border border-line px-4 py-1.5 text-sm hover:bg-subtle disabled:opacity-60" :disabled="busy" @click="emit('cancel')">
          取消
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-ctl bg-accent px-4 py-1.5 text-sm text-accent-fg hover:bg-accent-hover disabled:opacity-60"
          :disabled="busy"
          @click="emit('confirm')"
        >
          <LoaderCircle v-if="busy" class="size-4 animate-spin" />
          {{ busy ? "正在切换…" : "确定切换" }}
        </button>
      </div>
    </div>
  </div>
</template>
