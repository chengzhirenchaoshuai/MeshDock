<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { LoaderCircle, ShieldCheck, SlidersHorizontal } from "@lucide/vue";
import {
  getClientSetup,
  replaceOfficialTray,
  type ClientSetup,
  type LocalPrefs,
  type PrefSwitch,
} from "../api";
import UiCard from "./ui/UiCard.vue";
import UiSwitch from "./ui/UiSwitch.vue";

/** prefs 为 null 表示读不到（服务没运行等），开关全部禁用 */
const props = defineProps<{ prefs: LocalPrefs | null; busy: boolean }>();
/** on 是传给 tailscale 的值；ok 是成功后的提示语（按界面含义写） */
const emit = defineEmits<{ toggle: [pref: PrefSwitch, on: boolean, ok: string] }>();

/**
 * 界面上的开关。invert：界面含义与 tailscale 的参数相反
 * （“允许其他设备连接本机”打开 = shields-up 关闭，与官方客户端的 Allow incoming connections 一致）
 */
const items = computed(() => {
  const p = props.prefs;
  return [
    {
      pref: "shields-up" as const,
      invert: true,
      value: p?.shieldsUp,
      title: "允许组网里的其他设备连接本机",
      desc: "关闭后别的设备连不上本机，本机访问别人不受影响。远程控制本机时别关。",
    },
    {
      pref: "accept-dns" as const,
      invert: false,
      value: p?.acceptDns,
      title: "使用组网下发的 DNS",
      desc: "可直接用设备名（如 ming-pc）访问。注意别和代理软件的 DNS 冲突。",
    },
  ];
});

// ---------- 用 MeshDock 代替官方托盘：一次完成无人值守 + 取消托盘自启 + 退出托盘 ----------
// 无人值守和托盘自启不再单独显示（三者是一回事），远端设备的“官方客户端”面板里仍可单独调
const tray = ref<ClientSetup | null>(null);
const trayError = ref<string | null>(null);
const replacing = ref(false);

async function loadTray() {
  try {
    tray.value = await getClientSetup(null);
    trayError.value = null;
  } catch (e) {
    trayError.value = String(e);
  }
}
onMounted(loadTray);

/** 三项都到位才算已代替 */
const replaced = computed(() => !!tray.value && tray.value.unattended && !tray.value.trayAutostart && !tray.value.trayRunning);

async function toggleReplace(on: boolean) {
  replacing.value = true;
  trayError.value = null;
  try {
    tray.value = await replaceOfficialTray(on);
  } catch (e) {
    const msg = String(e);
    // 可能只完成了一部分，重新读取实际状态；读取本身的错误不盖掉这次的原因
    await loadTray();
    trayError.value = msg;
  } finally {
    replacing.value = false;
  }
}

/** 官方托盘不开机自启、又没开无人值守：开机后组网可能不会自动连接 */
const trayWarn = computed(() => !!tray.value && !tray.value.unattended && !tray.value.trayAutostart);
</script>

<template>
  <UiCard title="组网选项" :icon="SlidersHorizontal">
    <template #actions>
      <span class="ml-auto text-xs text-muted">{{ prefs ? "修改后立即生效" : "读取不到当前设置" }}</span>
      <LoaderCircle v-if="busy" class="size-4 animate-spin text-muted" />
    </template>
    <div class="flex flex-col divide-y divide-line">
      <label
        v-for="(it, i) in items"
        :key="it.pref"
        class="flex items-center gap-4"
        :class="[i === 0 ? 'pb-3' : 'py-3', prefs ? 'cursor-pointer' : '']"
      >
        <div class="min-w-0 flex-1">
          <p class="text-sm">{{ it.title }}</p>
          <p class="mt-0.5 text-xs text-muted">{{ it.desc }}</p>
        </div>
        <UiSwitch
          :model-value="it.value == null ? false : it.invert ? !it.value : it.value"
          :disabled="busy || it.value == null"
          @update:model-value="emit('toggle', it.pref, it.invert ? !$event : $event, `已${$event ? '开启' : '关闭'}“${it.title}”`)"
        />
      </label>
      <label class="flex items-center gap-4 pt-3" :class="{ 'cursor-pointer': !!tray }">
        <div class="min-w-0 flex-1">
          <p class="flex items-center gap-1.5 text-sm">
            用 MeshDock 代替官方托盘
            <ShieldCheck class="size-3.5 text-faint" title="需要管理员权限" />
          </p>
          <p class="mt-0.5 text-xs text-muted">
            开启后退出官方托盘并取消它的开机自启，没有用户登录时组网也保持连接。关闭则恢复官方托盘。修改会弹出管理员确认。
          </p>
          <p v-if="trayError" class="mt-1 text-xs break-words text-error">{{ trayError }}</p>
          <p v-else-if="trayWarn" class="mt-1 text-xs text-punching">
            官方托盘不开机自启，开机后组网可能不会自动连接，建议打开这一项。
          </p>
        </div>
        <LoaderCircle v-if="replacing || (!tray && !trayError)" class="size-4 animate-spin text-muted" />
        <UiSwitch :model-value="replaced" :disabled="replacing || busy || !tray" @update:model-value="toggleReplace" />
      </label>
    </div>
  </UiCard>
</template>
