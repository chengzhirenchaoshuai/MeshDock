<script setup lang="ts">
import { ref } from "vue";
import { LoaderCircle, TriangleAlert } from "@lucide/vue";
import type { CliError } from "../api";

/** 断开组网的二次确认：弹窗 + 勾选“我知道会中断”后才能确定 */
defineProps<{ busy: boolean; error: CliError | null }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();

const acknowledged = ref(false);
</script>

<template>
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/30 p-6" @click.self="!busy && emit('cancel')">
    <div class="flex w-full max-w-md flex-col gap-4 rounded-card border border-line bg-surface p-5 shadow-card">
      <h2 class="flex items-center gap-2 text-base font-semibold">
        <TriangleAlert class="size-5" style="color: var(--md-error)" />
        断开组网？
      </h2>
      <p class="text-sm text-muted">
        将执行 <code class="num rounded bg-subtle px-1">tailscale down</code>。断开后：
      </p>
      <ul class="list-disc space-y-1 pl-5 text-sm leading-relaxed">
        <li>本机退出组网，访问不到其他设备。</li>
        <li><strong>正在经组网连到这台电脑的 SSH、RustDesk 会话会立即中断。</strong></li>
        <li>如果你正在别处通过组网远程操作这台电脑，断开后就连不回来了，只能在这台电脑上重新连接。</li>
      </ul>
      <label class="inline-flex cursor-pointer items-center gap-2 text-sm">
        <input v-model="acknowledged" type="checkbox" class="size-4 accent-[var(--md-error)]" />
        我知道会中断以上连接
      </label>
      <details v-if="error" class="text-sm" open>
        <summary class="cursor-pointer select-none" style="color: var(--md-error)">{{ error.message }}</summary>
        <pre class="num mt-2 overflow-auto rounded-ctl bg-subtle p-2 text-xs whitespace-pre-wrap select-text">{{ error.raw }}</pre>
      </details>
      <div class="flex justify-end gap-2">
        <button
          class="rounded-ctl border border-line px-4 py-1.5 text-sm hover:bg-subtle disabled:opacity-60"
          :disabled="busy"
          @click="emit('cancel')"
        >
          取消
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-ctl px-4 py-1.5 text-sm text-white disabled:cursor-not-allowed disabled:opacity-50"
          style="background: var(--md-error)"
          :disabled="busy || !acknowledged"
          @click="emit('confirm')"
        >
          <LoaderCircle v-if="busy" class="size-4 animate-spin" />
          {{ busy ? "正在断开…" : "断开" }}
        </button>
      </div>
    </div>
  </div>
</template>
