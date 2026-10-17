<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Check, Gauge, LoaderCircle, Network, RefreshCw, TriangleAlert } from "@lucide/vue";
import {
  getSettings,
  getSnapshot,
  onModeRequest,
  onSnapshot,
  ping,
  refresh,
  saveSettings,
  setAcceptRoutes,
  takeModeRequest,
  type CliError,
  type Device,
  type PingRecord,
  type Settings,
  type Snapshot,
} from "./api";
import { backendLabel, clock } from "./format";
import DeviceCard from "./components/DeviceCard.vue";
import ErrorView from "./components/ErrorView.vue";
import ModeConfirm from "./components/ModeConfirm.vue";
import ModePanel from "./components/ModePanel.vue";
import NetcheckPanel from "./components/NetcheckPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";

const tabs = [
  { id: "devices", label: "设备" },
  { id: "netcheck", label: "网络体检" },
  { id: "settings", label: "设置" },
] as const;
const tab = ref<(typeof tabs)[number]["id"]>("devices");

const snapshot = ref<Snapshot | null>(null);
const busy = ref(false);
/** 当前时间，每 5 秒更新：用于“X 分钟前”和测试结果的 30 秒覆盖 */
const now = ref(Date.now());
const toast = ref<string | null>(null);
/** 设备 id → 最近一次测试结果（只存在内存里） */
const pings = ref<Record<string, PingRecord>>({});
/** 正在测试的设备 id */
const testing = ref<Set<string>>(new Set());

const overview = computed(() => snapshot.value?.overview ?? null);
const peers = computed(() => overview.value?.peers ?? []);
const onlineCount = computed(() => peers.value.filter((p) => p.conn.kind !== "offline").length);

/** 顶部的整体状态：文字 + 状态色 */
const overall = computed(() => {
  const o = overview.value;
  if (!o) return { label: "出错", tone: "error" };
  if (o.backendState !== "Running") return { label: backendLabel(o.backendState), tone: "offline" };
  if (o.controlUnreachable) return { label: "控制端连不上", tone: "error" };
  return { label: "已连接", tone: "direct" };
});

/** 1.102.4-t1a2b3c-g4d5e6f → 1.102.4 */
const shortVersion = (v: string | null) => (v ? v.split("-")[0] : "版本未知");

async function doRefresh() {
  if (busy.value) return;
  busy.value = true;
  try {
    snapshot.value = await refresh();
  } finally {
    busy.value = false;
  }
}

/** 设备的组网 IPv4，测试用 */
const deviceIp = (d: Device) => d.ips.find((a) => !a.includes(":")) ?? d.ips[0];

async function testOne(d: Device) {
  const ip = deviceIp(d);
  if (!ip || testing.value.has(d.id)) return;
  testing.value = new Set(testing.value).add(d.id);
  let record: PingRecord;
  try {
    record = { result: await ping(ip), error: null, atMs: Date.now() };
  } catch (e) {
    record = { result: null, error: e as CliError, atMs: Date.now() };
  }
  pings.value = { ...pings.value, [d.id]: record };
  now.value = Date.now();
  const next = new Set(testing.value);
  next.delete(d.id);
  testing.value = next;
}

/** 并发测试所有在线设备，离线的跳过 */
const testablePeers = computed(() => peers.value.filter((p) => p.conn.kind !== "offline"));
const testingAll = computed(() => testing.value.size > 0);
const testAll = () => Promise.all(testablePeers.value.map(testOne));

let toastTimer: number | undefined;
function showToast(text: string) {
  toast.value = text;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = null), 2500);
}

async function copyIp(ip: string) {
  try {
    await navigator.clipboard.writeText(ip);
    showToast(`已复制 ${ip}`);
  } catch {
    showToast("复制失败");
  }
}

// ---------- 在家 / 出门模式 ----------
const settings = ref<Settings | null>(null);
const modeBusy = ref(false);
/** 等待确认的目标模式：true 出门、false 在家、null 没有弹框 */
const confirmOn = ref<boolean | null>(null);
const modeError = ref<CliError | null>(null);

function requestMode(on: boolean) {
  modeError.value = null;
  confirmOn.value = on;
}

/** 执行切换；fromDialog 为 false 时（一键改回）错误用提示条显示 */
async function switchMode(on: boolean, fromDialog: boolean) {
  modeBusy.value = true;
  try {
    snapshot.value = await setAcceptRoutes(on);
    confirmOn.value = null;
    showToast(`已切换到${on ? "出门模式" : "在家模式"}`);
  } catch (e) {
    if (fromDialog) modeError.value = e as CliError;
    else showToast(`切换失败：${(e as CliError).message}`);
  } finally {
    modeBusy.value = false;
  }
}

/** 读不到网关时的手动标记 */
function updateAtHome(atHome: true | null) {
  if (settings.value) updateSettings({ ...settings.value, atHome });
}

async function updateSettings(next: Settings) {
  try {
    settings.value = await saveSettings(next);
  } catch (e) {
    showToast(`保存失败：${String(e)}`);
  }
}

/** 托盘点了模式菜单：切到设备页并弹出确认框 */
async function takeTrayRequest() {
  const on = await takeModeRequest();
  if (on === null) return;
  tab.value = "devices";
  requestMode(on);
}

const unlisteners: (() => void)[] = [];
let clockTimer: number | undefined;
onMounted(async () => {
  unlisteners.push(await onSnapshot((s) => (snapshot.value = s)));
  unlisteners.push(await onModeRequest(takeTrayRequest));
  clockTimer = window.setInterval(() => (now.value = Date.now()), 5_000);
  settings.value = await getSettings();
  snapshot.value = await getSnapshot();
  // 窗口是被托盘的模式菜单唤起时，请求已经在等着了
  await takeTrayRequest();
});
onUnmounted(() => {
  unlisteners.forEach((f) => f());
  clearInterval(clockTimer);
});
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- 顶栏 -->
    <header class="flex items-center gap-3 border-b border-line bg-surface px-6 py-3">
      <Network class="size-5 text-accent" />
      <h1 class="text-base font-semibold">MeshDock</h1>
      <span
        v-if="snapshot"
        class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium"
        :style="{
          color: `var(--md-${overall.tone})`,
          background: `color-mix(in srgb, var(--md-${overall.tone}) 13%, transparent)`,
        }"
      >
        <span class="size-1.5 rounded-full" :style="{ background: `var(--md-${overall.tone})` }" />
        {{ overall.label }}
      </span>
      <span v-if="overview" class="text-xs text-muted">
        {{ peers.length }} 台设备 · {{ onlineCount }} 台在线
      </span>
      <span v-if="snapshot" class="num ml-auto text-xs text-faint">
        更新于 {{ clock(snapshot.updatedAtMs) }}
      </span>
      <button
        v-if="overview && tab === 'devices'"
        class="inline-flex items-center gap-1.5 rounded-ctl border border-line px-3 py-1 text-sm hover:bg-subtle disabled:opacity-60"
        :disabled="testingAll || testablePeers.length === 0"
        title="测试所有在线设备的实际路径和延迟"
        @click="testAll"
      >
        <LoaderCircle v-if="testingAll" class="size-4 animate-spin" />
        <Gauge v-else class="size-4" />
        全部测试
      </button>
      <button
        class="inline-flex items-center gap-1.5 rounded-ctl border border-line px-3 py-1 text-sm hover:bg-subtle disabled:opacity-60"
        :class="{ 'ml-auto': !snapshot }"
        :disabled="busy"
        @click="doRefresh"
      >
        <RefreshCw class="size-4" :class="{ 'animate-spin': busy }" />
        刷新
      </button>
    </header>

    <!-- 标签页 -->
    <nav class="flex gap-1 border-b border-line bg-surface px-4">
      <button
        v-for="t in tabs"
        :key="t.id"
        class="relative px-3 py-2 text-sm"
        :class="tab === t.id ? 'font-medium text-fg' : 'text-muted hover:text-fg'"
        @click="tab = t.id"
      >
        {{ t.label }}
        <span v-if="tab === t.id" class="absolute inset-x-3 -bottom-px h-0.5 rounded-full bg-accent" />
      </button>
    </nav>

    <!-- 网络体检（与组网状态无关，出错时也能用） -->
    <main v-show="tab === 'netcheck'" class="flex-1 overflow-y-auto px-6 py-5">
      <NetcheckPanel />
    </main>

    <main v-show="tab === 'settings'" class="flex-1 overflow-y-auto px-6 py-5">
      <SettingsPanel :settings="settings" :gateways="overview?.location.gateways ?? []" @save="updateSettings" />
    </main>

    <main v-show="tab === 'devices'" class="flex-1 overflow-y-auto px-6 py-5">
      <!-- 加载中 -->
      <div v-if="!snapshot" class="flex h-full items-center justify-center gap-2 text-muted">
        <LoaderCircle class="size-5 animate-spin" />
        正在读取组网状态…
      </div>

      <!-- 出错 -->
      <ErrorView v-else-if="snapshot.error" :error="snapshot.error" :busy="busy" @retry="doRefresh" />

      <template v-else-if="overview">
        <ModePanel
          class="mb-6"
          :overview="overview"
          :settings="settings"
          :busy="modeBusy"
          @request="requestMode"
          @fix-home="switchMode(false, false)"
          @set-at-home="updateAtHome"
        />

        <!-- 本机 -->
        <section v-if="overview.selfDevice" class="mb-6">
          <h2 class="mb-2 text-xs font-medium text-muted">本机</h2>
          <div class="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4">
            <DeviceCard
              :device="overview.selfDevice"
              :now="now"
              :note="`Tailscale 官方客户端 ${shortVersion(overview.version)}`"
              @copy="copyIp"
            />
          </div>
        </section>

        <!-- 其他设备 -->
        <section>
          <h2 class="mb-2 text-xs font-medium text-muted">组网设备</h2>
          <p v-if="peers.length === 0" class="py-12 text-center text-muted">
            组网里还没有其他设备
          </p>
          <div v-else class="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4">
            <DeviceCard
              v-for="d in peers"
              :key="d.id"
              :device="d"
              :now="now"
              :ping="pings[d.id]"
              :testing="testing.has(d.id)"
              @copy="copyIp"
              @test="testOne(d)"
            />
          </div>
        </section>
      </template>
    </main>

    <ModeConfirm
      v-if="confirmOn !== null"
      :on="confirmOn"
      :at-home="overview?.location.atHome ?? null"
      :busy="modeBusy"
      :error="modeError"
      @confirm="switchMode(confirmOn, true)"
      @cancel="confirmOn = null"
    />

    <!-- 操作提示 -->
    <Transition
      enter-from-class="opacity-0 translate-y-2"
      leave-to-class="opacity-0 translate-y-2"
      enter-active-class="transition duration-200"
      leave-active-class="transition duration-200"
    >
      <div
        v-if="toast"
        class="fixed bottom-5 left-1/2 inline-flex -translate-x-1/2 items-center gap-2 rounded-full bg-fg px-4 py-2 text-sm text-bg shadow-card"
      >
        <TriangleAlert v-if="toast.includes('失败')" class="size-4" />
        <Check v-else class="size-4" />
        {{ toast }}
      </div>
    </Transition>
  </div>
</template>
