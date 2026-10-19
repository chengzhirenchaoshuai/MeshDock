<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { BatteryCharging, BatteryFull, ChevronDown, Clock, Cpu, Globe, Plug, RefreshCw, Server } from "@lucide/vue";
import { getSysinfo, type SysInfo } from "../api";
import { formatBytes, formatUptime, timeAgo, usageTone } from "../format";
import UiButton from "./ui/UiButton.vue";

/** 展开时多久自动重新采集一次 */
const AUTO_REFRESH_MS = 5 * 60_000;

/** alias：ssh 别名；deviceId：用来记住每台设备的展开状态 */
const props = defineProps<{ alias: string; deviceId: string; now: number; offline: boolean }>();

const info = ref<SysInfo | null>(null);
const error = ref<string | null>(null);
const loading = ref(false);
/** 最近一次成功采集的时刻 */
const atMs = ref(0);
/** 最近一次尝试采集的时刻（不论成败），自动刷新按它计间隔，失败后不会反复重连 */
let triedMs = 0;

// 展开状态按设备记在本机，读写失败（如被禁用）时默认收起
const OPEN_KEY = `meshdock.sysinfo.open.${props.deviceId}`;
const open = ref(readOpen());
function readOpen() {
  try {
    return localStorage.getItem(OPEN_KEY) === "1";
  } catch {
    return false;
  }
}
watch(open, (v) => {
  try {
    if (v) localStorage.setItem(OPEN_KEY, "1");
    else localStorage.removeItem(OPEN_KEY);
  } catch {
    /* 记不住只影响下次打开时的展开状态 */
  }
});

async function load() {
  if (loading.value || props.offline) return;
  loading.value = true;
  triedMs = Date.now();
  try {
    info.value = await getSysinfo(props.alias);
    atMs.value = Date.now();
    error.value = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

// 展开时第一次采集；之后随 now 的走动，距上次尝试满 5 分钟再采集（失败也一样等 5 分钟，可手动刷新）
watch(
  () => [open.value, props.now, props.offline] as const,
  ([isOpen]) => {
    if (!isOpen || loading.value || props.offline) return;
    if (Date.now() - triedMs > AUTO_REFRESH_MS) load();
  },
  { immediate: true },
);

const pct = (used: number, total: number) => (total > 0 ? (used / total) * 100 : 0);

/** 用量条：内存、Swap、各分区 */
const meters = computed(() => {
  const i = info.value;
  if (!i) return [];
  const list: { label: string; pct: number; text: string }[] = [];
  if (i.memTotalKb != null && i.memAvailableKb != null) {
    const used = i.memTotalKb - i.memAvailableKb;
    list.push({ label: "内存", pct: pct(used, i.memTotalKb), text: `${formatBytes(used * 1024)} / ${formatBytes(i.memTotalKb * 1024)}` });
  }
  if (i.swapTotalKb && i.swapFreeKb != null) {
    const used = i.swapTotalKb - i.swapFreeKb;
    list.push({ label: "Swap", pct: pct(used, i.swapTotalKb), text: `${formatBytes(used * 1024)} / ${formatBytes(i.swapTotalKb * 1024)}` });
  }
  for (const d of i.disks) {
    list.push({ label: d.mount, pct: pct(d.usedKb, d.totalKb), text: `${formatBytes(d.usedKb * 1024)} / ${formatBytes(d.totalKb * 1024)}` });
  }
  return list;
});

/** 负载按核数折算：1 分钟负载超过核数时提醒 */
const loadBusy = computed(() => {
  const i = info.value;
  return !!(i?.load && i.cpus && i.load[0] >= i.cpus);
});

const BATTERY_STATUS: Record<string, string> = {
  Charging: "充电中",
  Discharging: "未充电",
  "Not charging": "未充电",
  Full: "已充满",
};
const batteryText = computed(() => {
  const b = info.value?.battery;
  if (!b) return null;
  return [
    b.percent != null ? `${b.percent}%` : null,
    b.status ? (BATTERY_STATUS[b.status] ?? b.status) : null,
    b.tempC != null ? `${b.tempC.toFixed(1)} ℃` : null,
  ]
    .filter(Boolean)
    .join(" · ");
});
/** 电量低于 20% 且没在充电，或温度 45 ℃ 以上时提醒 */
const batteryWarn = computed(() => {
  const b = info.value?.battery;
  if (!b) return false;
  const low = b.percent != null && b.percent < 20 && b.status !== "Charging";
  return low || (b.tempC != null && b.tempC >= 45);
});

/** 常见端口的用途，只用于显示 */
const PORT_NAMES: Record<number, string> = {
  22: "SSH",
  80: "HTTP",
  139: "Samba",
  443: "HTTPS",
  445: "Samba",
  3389: "远程桌面",
  5900: "VNC",
  21118: "RustDesk",
};

/** 收起时标题行显示的摘要 */
const summary = computed(() => {
  const i = info.value;
  if (!i) return null;
  const parts: string[] = [];
  if (i.uptimeSecs != null) parts.push(`运行 ${formatUptime(i.uptimeSecs)}`);
  const mem = meters.value.find((m) => m.label === "内存");
  if (mem) parts.push(`内存 ${Math.round(mem.pct)}%`);
  if (i.battery?.percent != null) parts.push(`电量 ${i.battery.percent}%`);
  return parts.join(" · ");
});
</script>

<template>
  <div class="overflow-hidden rounded-ctl border border-line">
    <button
      type="button"
      class="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-xs text-muted transition-colors hover:bg-subtle hover:text-fg"
      :title="open ? '收起系统信息' : `展开系统信息（经 ssh ${alias} 只读采集）`"
      @click="open = !open"
    >
      <Server class="size-3.5 shrink-0 text-accent" />
      <span class="font-medium text-fg">系统信息</span>
      <span v-if="!open && summary" class="num min-w-0 flex-1 truncate">{{ summary }}</span>
      <span v-else class="flex-1" />
      <ChevronDown class="size-3.5 shrink-0 transition-transform duration-200" :class="{ 'rotate-180': open }" />
    </button>

    <div v-if="open" class="flex flex-col gap-2.5 border-t border-line px-2.5 py-2.5 text-xs">
      <p v-if="offline && !info" class="text-faint">设备离线，暂时采集不到</p>
      <p v-else-if="error && !info" class="leading-relaxed break-words text-error">{{ error }}</p>
      <p v-else-if="!info" class="text-faint">正在经 ssh {{ alias }} 采集…</p>

      <template v-if="info">
        <!-- 用量条 -->
        <div v-if="meters.length" class="flex flex-col gap-1.5">
          <div v-for="m in meters" :key="m.label" class="flex items-center gap-2">
            <span class="w-11 shrink-0 truncate text-muted" :title="m.label">{{ m.label }}</span>
            <span class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-subtle">
              <span
                class="block h-full rounded-full transition-[width] duration-500"
                :style="{ width: `${Math.min(m.pct, 100)}%`, background: `var(--md-${usageTone(m.pct)})` }"
              />
            </span>
            <span class="num w-9 shrink-0 text-right" :style="{ color: `var(--md-${usageTone(m.pct)})` }">
              {{ Math.round(m.pct) }}%
            </span>
            <span class="num hidden w-28 shrink-0 text-right text-faint min-[420px]:inline">{{ m.text }}</span>
          </div>
        </div>

        <!-- 运行时长、负载、电池 -->
        <div class="flex flex-wrap gap-x-4 gap-y-1.5 text-muted">
          <span v-if="info.uptimeSecs != null" class="inline-flex items-center gap-1" title="距上次开机">
            <Clock class="size-3.5 text-faint" />运行 {{ formatUptime(info.uptimeSecs) }}
          </span>
          <span
            v-if="info.load"
            class="num inline-flex items-center gap-1"
            :class="{ 'text-punching': loadBusy }"
            :title="`1 / 5 / 15 分钟平均负载：${info.load.join(' / ')}${info.cpus ? `，共 ${info.cpus} 核` : ''}`"
          >
            <Cpu class="size-3.5 text-faint" />负载 {{ info.load[0].toFixed(2) }}<template v-if="info.cpus"> / {{ info.cpus }} 核</template>
          </span>
          <span
            v-if="batteryText"
            class="num inline-flex items-center gap-1"
            :class="{ 'text-punching': batteryWarn }"
            title="手机电池（电量 · 充电状态 · 温度）"
          >
            <component :is="info.battery?.status === 'Charging' ? BatteryCharging : BatteryFull" class="size-3.5 text-faint" />
            {{ batteryText }}
          </span>
        </div>

        <!-- IPv6：没有全局地址时直连只能靠 IPv4 打洞，容易退回中继 -->
        <div class="flex items-start gap-1">
          <Globe class="mt-0.5 size-3.5 shrink-0 text-faint" />
          <span v-if="info.ipv6.length" class="text-muted" :title="info.ipv6.join('\n')">
            IPv6 正常：{{ info.ipv6.length }} 个全局地址
          </span>
          <span v-else class="leading-relaxed text-punching">
            没有 IPv6 全局地址：和其他设备直连只能靠 IPv4 打洞，容易退回中继
          </span>
        </div>

        <!-- 监听端口 -->
        <div v-if="info.ports.length" class="flex items-start gap-1">
          <Plug class="mt-0.5 size-3.5 shrink-0 text-faint" />
          <span class="flex flex-wrap gap-1">
            <span
              v-for="p in info.ports"
              :key="p"
              class="num rounded bg-subtle px-1.5 text-muted"
              :title="PORT_NAMES[p] ? `${p}：${PORT_NAMES[p]}` : `TCP ${p}`"
            >
              {{ p }}<span v-if="PORT_NAMES[p]" class="text-faint"> {{ PORT_NAMES[p] }}</span>
            </span>
          </span>
        </div>

        <!-- 采集时间 + 刷新 -->
        <div class="flex items-center gap-2 text-faint">
          <span v-if="error" class="min-w-0 flex-1 truncate text-error" :title="error">刷新失败：{{ error }}</span>
          <span v-else class="flex-1">{{ timeAgo(atMs, now) }}采集，展开时每 5 分钟自动刷新</span>
          <UiButton
            variant="ghost"
            size="xs"
            :icon="RefreshCw"
            :loading="loading"
            :disabled="loading || offline"
            title="立即重新采集"
            @click="load"
          >
            刷新
          </UiButton>
        </div>
      </template>
      <div v-else-if="error && !offline" class="flex justify-end">
        <UiButton variant="ghost" size="xs" :icon="RefreshCw" :loading="loading" :disabled="loading" @click="load">
          重试
        </UiButton>
      </div>
    </div>
  </div>
</template>
