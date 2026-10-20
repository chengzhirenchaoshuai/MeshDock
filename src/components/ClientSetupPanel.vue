<script setup lang="ts">
import { ref, watch } from "vue";
import { ChevronDown, PowerOff, RefreshCw, Settings2 } from "@lucide/vue";
import { getClientSetup, setRemoteUnattended, setTrayAutostart, type ClientSetup } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiDialog from "./ui/UiDialog.vue";
import UiSwitch from "./ui/UiSwitch.vue";

/** 经 ssh 查看和修改 Windows 设备上官方客户端的无人值守与托盘自启。alias：ssh 别名；name：设备名 */
const props = defineProps<{ alias: string; name: string; offline: boolean }>();

const open = ref(false);
const setup = ref<ClientSetup | null>(null);
const error = ref<string | null>(null);
const loading = ref(false);
/** 正在修改的项 */
const busy = ref<"unattended" | "tray" | null>(null);
/** 等待确认关闭无人值守 */
const confirmOff = ref(false);

async function load() {
  if (loading.value || props.offline) return;
  loading.value = true;
  try {
    setup.value = await getClientSetup(props.alias);
    error.value = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

// 第一次展开时读取
watch(open, (v) => {
  if (v && !setup.value) load();
});

async function apply(item: "unattended" | "tray", on: boolean) {
  busy.value = item;
  error.value = null;
  try {
    setup.value = item === "unattended" ? await setRemoteUnattended(props.alias, on) : await setTrayAutostart(props.alias, on);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = null;
    confirmOff.value = false;
  }
}

/** 关闭无人值守可能让对方在没人登录时掉线、远程连不回去，先确认 */
function toggleUnattended(on: boolean) {
  if (on) apply("unattended", true);
  else confirmOff.value = true;
}
</script>

<template>
  <div class="overflow-hidden rounded-ctl border border-line">
    <button
      type="button"
      class="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-xs text-muted transition-colors hover:bg-subtle hover:text-fg"
      :title="open ? '收起' : `展开（经 ssh ${alias} 查看和修改官方客户端的启动方式）`"
      @click="open = !open"
    >
      <Settings2 class="size-3.5 shrink-0 text-accent" />
      <span class="font-medium text-fg">官方客户端</span>
      <span v-if="!open && setup" class="min-w-0 flex-1 truncate">
        无人值守{{ setup.unattended ? "开" : "关" }} · 托盘自启{{ setup.trayAutostart ? "开" : "关" }}
      </span>
      <span v-else class="flex-1" />
      <ChevronDown class="size-3.5 shrink-0 transition-transform duration-200" :class="{ 'rotate-180': open }" />
    </button>

    <div v-if="open" class="flex flex-col gap-2.5 border-t border-line px-2.5 py-2.5 text-xs">
      <p v-if="offline && !setup" class="text-faint">设备离线，暂时读取不到</p>
      <p v-else-if="!setup && !error" class="text-faint">正在经 ssh {{ alias }} 读取…</p>
      <p v-if="error" class="leading-relaxed break-words text-error">{{ error }}</p>

      <template v-if="setup">
        <label class="flex items-center gap-3" :class="{ 'cursor-pointer': !offline }">
          <span class="min-w-0 flex-1">
            <span class="block text-fg">没有用户登录时也保持连接（无人值守）</span>
            <span class="mt-0.5 block text-faint">开启后，对方注销、托盘退出或开机还没登录时，组网都保持连接</span>
          </span>
          <UiSwitch
            :model-value="setup.unattended"
            :disabled="!!busy || offline"
            @update:model-value="toggleUnattended"
          />
        </label>
        <label class="flex items-center gap-3" :class="{ 'cursor-pointer': setup.admin && setup.shortcut && !offline }">
          <span class="min-w-0 flex-1">
            <span class="block text-fg">开机时启动官方托盘图标</span>
            <span v-if="!setup.shortcut" class="mt-0.5 block text-faint">对方启动文件夹里没有官方托盘的快捷方式</span>
            <span v-else-if="!setup.admin" class="mt-0.5 block text-faint">SSH 登录的账户不是管理员，不能修改</span>
            <span v-else-if="!setup.trayAutostart && !setup.unattended" class="mt-0.5 block text-punching">
              无人值守未开启，开机后组网可能要等打开官方客户端才会连接
            </span>
            <span v-else class="mt-0.5 block text-faint">开启了无人值守时可以关掉，组网照样开机自动连接</span>
          </span>
          <UiSwitch
            :model-value="setup.trayAutostart"
            :disabled="!!busy || offline || !setup.admin || !setup.shortcut"
            @update:model-value="apply('tray', $event)"
          />
        </label>
      </template>

      <div v-if="setup || error" class="flex justify-end">
        <UiButton size="sm" :icon="RefreshCw" :loading="loading || !!busy" :disabled="loading || !!busy || offline" @click="load">
          重新读取
        </UiButton>
      </div>
    </div>

    <UiDialog
      v-if="confirmOff"
      :title="`关闭 ${name} 的无人值守？`"
      :icon="PowerOff"
      tone="punching"
      :dismissible="busy === null"
      @close="confirmOff = false"
    >
      <p class="text-sm leading-relaxed text-muted">
        关闭后，{{ name }} 上没有用户登录（注销、重启后停在登录界面）或官方托盘退出时，组网会断开，
        到时经组网的 SSH、远程桌面都连不上，只能到那台电脑前重新登录。
      </p>
      <template #footer>
        <UiButton :disabled="busy !== null" @click="confirmOff = false">取消</UiButton>
        <UiButton
          variant="primary"
          :loading="busy === 'unattended'"
          :disabled="busy !== null"
          @click="apply('unattended', false)"
        >
          确定关闭
        </UiButton>
      </template>
    </UiDialog>
  </div>
</template>
