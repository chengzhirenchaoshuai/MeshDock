<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { LoaderCircle, ShieldCheck, SlidersHorizontal } from "@lucide/vue";
import { getClientSetup, setTrayAutostart, type ClientSetup, type LocalPrefs, type PrefSwitch } from "../api";
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
    {
      pref: "unattended" as const,
      invert: false,
      value: p?.unattended,
      title: "没有用户登录时也保持连接",
      desc: "开启后可退出官方托盘，未登录、注销时组网也不断开。",
    },
  ];
});

// ---------- 官方托盘的开机自启（改的是 Windows 启动项，不是 tailscale 的设置） ----------
const tray = ref<ClientSetup | null>(null);
const trayError = ref<string | null>(null);
const trayBusy = ref(false);

async function loadTray() {
  try {
    tray.value = await getClientSetup(null);
    trayError.value = null;
  } catch (e) {
    trayError.value = String(e);
  }
}
onMounted(loadTray);

async function toggleTray(on: boolean) {
  trayBusy.value = true;
  trayError.value = null;
  try {
    tray.value = await setTrayAutostart(null, on);
  } catch (e) {
    trayError.value = String(e);
  } finally {
    trayBusy.value = false;
  }
}

/** 没开无人值守却关掉了托盘自启：开机后组网可能要等有人打开官方客户端才连接 */
const trayWarn = computed(() => tray.value?.trayAutostart === false && props.prefs?.unattended === false);
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
      <label class="flex items-center gap-4 pt-3" :class="{ 'cursor-pointer': tray?.shortcut }">
        <div class="min-w-0 flex-1">
          <p class="flex items-center gap-1.5 text-sm">
            开机时启动官方托盘图标
            <ShieldCheck class="size-3.5 text-faint" title="需要管理员权限" />
          </p>
          <p class="mt-0.5 text-xs text-muted">已开启上一项时可关掉，组网照样开机连接。修改会弹出管理员确认。</p>
          <p v-if="trayError" class="mt-1 text-xs break-words text-error">{{ trayError }}</p>
          <p v-else-if="tray && !tray.shortcut" class="mt-1 text-xs text-faint">没找到官方托盘的启动项，无法开关</p>
          <p v-else-if="trayWarn" class="mt-1 text-xs text-punching">
            上一项未开启，开机后组网可能不会自动连接，建议先开启。
          </p>
        </div>
        <LoaderCircle v-if="trayBusy || (!tray && !trayError)" class="size-4 animate-spin text-muted" />
        <UiSwitch
          :model-value="tray?.trayAutostart ?? false"
          :disabled="trayBusy || !tray?.shortcut"
          @update:model-value="toggleTray"
        />
      </label>
    </div>
  </UiCard>
</template>
