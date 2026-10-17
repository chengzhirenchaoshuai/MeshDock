<script setup lang="ts">
import { House, Plane } from "@lucide/vue";
import type { CliError } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiCallout from "./ui/UiCallout.vue";
import UiDialog from "./ui/UiDialog.vue";

/** on：要切到的模式（true 出门、false 在家） */
defineProps<{ on: boolean; atHome: boolean | null; busy: boolean; error: CliError | null }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <UiDialog
    :title="`切换到${on ? '出门模式' : '在家模式'}？`"
    :icon="on ? Plane : House"
    :dismissible="!busy"
    @close="emit('cancel')"
  >
    <p class="text-sm leading-relaxed text-muted">
      将执行 <code class="ui-code num px-1.5 py-0.5 text-xs text-fg">tailscale set --accept-routes={{ on }}</code>。
      {{ on ? "开启后可以通过子网路由访问家里局域网的设备。" : "关闭后访问家里局域网不再经过隧道。" }}
    </p>
    <UiCallout v-if="on && atHome === true" tone="error">
      你现在在家。在家开启出门模式会让本地流量绕进隧道，可能断网，确定吗？
    </UiCallout>
    <details v-if="error" class="text-sm" open>
      <summary class="cursor-pointer text-error select-none">{{ error.message }}</summary>
      <pre class="ui-code num mt-2 overflow-auto p-2 text-xs whitespace-pre-wrap select-text">{{ error.raw }}</pre>
    </details>
    <template #footer>
      <UiButton :disabled="busy" @click="emit('cancel')">取消</UiButton>
      <UiButton variant="primary" :loading="busy" :disabled="busy" @click="emit('confirm')">
        {{ busy ? "正在切换…" : "确定切换" }}
      </UiButton>
    </template>
  </UiDialog>
</template>
