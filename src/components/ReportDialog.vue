<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Copy, Download, LoaderCircle, X } from "@lucide/vue";
import { buildReport, saveReport } from "../api";

const emit = defineEmits<{ close: [] }>();

const text = ref<string | null>(null);
const error = ref<string | null>(null);
/** 操作结果提示，例如“已复制”“已保存到 …” */
const notice = ref<string | null>(null);

onMounted(async () => {
  try {
    text.value = await buildReport();
  } catch (e) {
    error.value = `生成失败：${String(e)}`;
  }
});

async function copy() {
  if (!text.value) return;
  try {
    await navigator.clipboard.writeText(text.value);
    notice.value = "已复制到剪贴板";
  } catch {
    notice.value = "复制失败";
  }
}

async function save() {
  if (!text.value) return;
  try {
    notice.value = `已保存：${await saveReport(text.value)}`;
  } catch (e) {
    notice.value = String(e);
  }
}
</script>

<template>
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/30 p-6" @click.self="emit('close')">
    <div class="flex max-h-full w-full max-w-3xl flex-col gap-3 rounded-card border border-line bg-surface p-5 shadow-card">
      <header class="flex items-center gap-2">
        <h2 class="text-base font-semibold">诊断报告</h2>
        <span class="text-xs text-muted">公网 IP 只保留第一段，节点密钥只保留前 8 位，可以放心发给运维</span>
        <button class="ml-auto rounded-ctl p-1 text-muted hover:bg-subtle" title="关闭" @click="emit('close')">
          <X class="size-4" />
        </button>
      </header>

      <div v-if="!text && !error" class="flex items-center justify-center gap-2 py-16 text-muted">
        <LoaderCircle class="size-5 animate-spin" />
        正在收集状态和体检结果，约 5 秒…
      </div>
      <p v-else-if="error" class="py-8 text-center text-error">{{ error }}</p>
      <pre
        v-else
        class="num min-h-0 flex-1 overflow-auto rounded-ctl bg-subtle p-3 text-xs leading-relaxed whitespace-pre-wrap select-text"
      >{{ text }}</pre>

      <footer class="flex items-center gap-2">
        <span class="min-w-0 flex-1 truncate text-xs text-muted" :title="notice ?? undefined">{{ notice }}</span>
        <button
          class="inline-flex items-center gap-1.5 rounded-ctl border border-line px-3 py-1.5 text-sm hover:bg-subtle disabled:opacity-50"
          :disabled="!text"
          @click="copy"
        >
          <Copy class="size-4" />复制
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-ctl bg-accent px-3 py-1.5 text-sm text-accent-fg hover:bg-accent-hover disabled:opacity-50"
          :disabled="!text"
          @click="save"
        >
          <Download class="size-4" />保存到下载文件夹
        </button>
      </footer>
    </div>
  </div>
</template>
