<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Copy, Download, FileText, LoaderCircle } from "@lucide/vue";
import { buildReport, saveReport } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiDialog from "./ui/UiDialog.vue";

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
  <UiDialog
    title="诊断报告"
    subtitle="公网 IP 只保留第一段，节点密钥只保留前 8 位，可以放心发给运维"
    :icon="FileText"
    width="3xl"
    close-button
    @close="emit('close')"
  >
    <div v-if="!text && !error" class="flex items-center justify-center gap-2 py-16 text-muted">
      <LoaderCircle class="size-5 animate-spin" />
      正在收集状态和体检结果，约 5 秒…
    </div>
    <p v-else-if="error" class="py-8 text-center text-error">{{ error }}</p>
    <pre
      v-else
      class="ui-code num min-h-0 flex-1 overflow-auto p-3 text-xs leading-relaxed whitespace-pre-wrap select-text"
    >{{ text }}</pre>

    <template #footer>
      <span class="min-w-0 flex-1 truncate text-xs text-muted" :title="notice ?? undefined">{{ notice }}</span>
      <UiButton size="sm" :icon="Copy" :disabled="!text" @click="copy">复制</UiButton>
      <UiButton size="sm" variant="primary" :icon="Download" :disabled="!text" @click="save">保存到下载文件夹</UiButton>
    </template>
  </UiDialog>
</template>
