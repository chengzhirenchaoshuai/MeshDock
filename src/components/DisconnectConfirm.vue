<script setup lang="ts">
import { ref } from "vue";
import { Unplug } from "@lucide/vue";
import type { CliError } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiDialog from "./ui/UiDialog.vue";

/** 断开组网的二次确认：弹窗 + 勾选“我知道会中断”后才能确定 */
defineProps<{ busy: boolean; error: CliError | null }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();

const acknowledged = ref(false);
</script>

<template>
  <UiDialog title="断开组网？" :icon="Unplug" tone="error" :dismissible="!busy" @close="emit('cancel')">
    <p class="text-sm text-muted">
      将执行 <code class="ui-code num px-1.5 py-0.5 text-xs text-fg">tailscale down</code>。断开后：
    </p>
    <ul class="list-disc space-y-1 pl-5 text-sm leading-relaxed marker:text-faint">
      <li>本机退出组网，访问不到其他设备。</li>
      <li><strong>正在经组网连到这台电脑的 SSH、RustDesk 会话会立即中断。</strong></li>
      <li>如果你正在别处通过组网远程操作这台电脑，断开后就连不回来了，只能在这台电脑上重新连接。</li>
    </ul>
    <label class="inline-flex cursor-pointer items-center gap-2 text-sm">
      <input v-model="acknowledged" type="checkbox" class="size-4 accent-[var(--md-error)]" />
      我知道会中断以上连接
    </label>
    <details v-if="error" class="text-sm" open>
      <summary class="cursor-pointer text-error select-none">{{ error.message }}</summary>
      <pre class="ui-code num mt-2 overflow-auto p-2 text-xs whitespace-pre-wrap select-text">{{ error.raw }}</pre>
    </details>
    <template #footer>
      <UiButton :disabled="busy" @click="emit('cancel')">取消</UiButton>
      <UiButton variant="danger" :loading="busy" :disabled="busy || !acknowledged" @click="emit('confirm')">
        {{ busy ? "正在断开…" : "断开" }}
      </UiButton>
    </template>
  </UiDialog>
</template>
