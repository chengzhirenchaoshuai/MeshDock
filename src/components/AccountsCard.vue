<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ArrowLeftRight, CircleCheck, LoaderCircle, Users } from "@lucide/vue";
import { getProfiles, switchProfile, type CliError, type Profile, type Snapshot } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiCard from "./ui/UiCard.vue";
import UiDialog from "./ui/UiDialog.vue";

const emit = defineEmits<{ switched: [snapshot: Snapshot, name: string] }>();

const profiles = ref<Profile[] | null>(null);
const loadError = ref<string | null>(null);
/** 等待确认切换的账户 */
const target = ref<Profile | null>(null);
const busy = ref(false);
const switchError = ref<CliError | null>(null);

async function load() {
  try {
    profiles.value = await getProfiles();
    loadError.value = null;
  } catch (e) {
    loadError.value = (e as CliError).message;
  }
}
onMounted(load);

const label = (p: Profile) => `${p.account || p.nickname}（${p.tailnet}）`;

function ask(p: Profile) {
  switchError.value = null;
  target.value = p;
}

async function doSwitch() {
  const p = target.value;
  if (!p) return;
  busy.value = true;
  try {
    const snapshot = await switchProfile(p.id);
    target.value = null;
    emit("switched", snapshot, label(p));
    await load();
  } catch (e) {
    switchError.value = e as CliError;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <UiCard title="账户" :icon="Users">
    <template #actions>
      <span class="ml-auto text-xs text-muted">添加或退出账户请用官方客户端</span>
    </template>
    <p v-if="loadError" class="text-xs text-error">读取账户失败：{{ loadError }}</p>
    <p v-else-if="!profiles" class="flex items-center gap-2 text-xs text-muted">
      <LoaderCircle class="size-3.5 animate-spin" />正在读取…
    </p>
    <ul v-else class="flex flex-col divide-y divide-line">
      <li v-for="p in profiles" :key="p.id" class="flex items-center gap-3 py-2 first:pt-0 last:pb-0">
        <div class="min-w-0 flex-1">
          <p class="flex items-center gap-1.5 text-sm">
            {{ p.account || p.nickname }}
            <span v-if="p.selected" class="inline-flex items-center gap-1 text-xs font-medium text-accent">
              <CircleCheck class="size-3.5" />当前
            </span>
          </p>
          <p class="num mt-0.5 truncate text-xs text-muted" :title="p.tailnet">组网（控制端）：{{ p.tailnet }}</p>
        </div>
        <UiButton v-if="!p.selected" size="sm" :icon="ArrowLeftRight" @click="ask(p)">切换</UiButton>
      </li>
    </ul>
    <p v-if="profiles && profiles.length === 1" class="mt-3 border-t border-line pt-3 text-xs text-muted">
      只登录了一个账户。同时用两个组网（例如家里和公司各一个 Headscale）时，可在官方客户端中再登录一个账户，之后在这里一键切换。
    </p>

    <UiDialog
      v-if="target"
      :title="`切换到 ${label(target)}？`"
      :icon="ArrowLeftRight"
      :dismissible="!busy"
      @close="target = null"
    >
      <p class="text-sm leading-relaxed text-muted">
        将执行 <code class="ui-code num px-1.5 py-0.5 text-xs text-fg">tailscale switch {{ target.id }}</code>。
        切换时会先断开当前组网，经当前组网的远程连接（SSH、远程桌面等）会中断；设备列表会换成另一个组网的设备。
      </p>
      <details v-if="switchError" class="text-sm" open>
        <summary class="cursor-pointer text-error select-none">{{ switchError.message }}</summary>
        <pre class="ui-code num mt-2 overflow-auto p-2 text-xs whitespace-pre-wrap select-text">{{ switchError.raw }}</pre>
      </details>
      <template #footer>
        <UiButton :disabled="busy" @click="target = null">取消</UiButton>
        <UiButton variant="primary" :loading="busy" :disabled="busy" @click="doSwitch">
          {{ busy ? "正在切换…" : "确定切换" }}
        </UiButton>
      </template>
    </UiDialog>
  </UiCard>
</template>
