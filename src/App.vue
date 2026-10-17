<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  Check,
  Gauge,
  LayoutGrid,
  LoaderCircle,
  Network,
  Plug,
  RefreshCw,
  Settings2,
  Stethoscope,
  TriangleAlert,
  Unplug,
} from "@lucide/vue";
import {
  connect,
  disconnect,
  getHistory,
  getSettings,
  getSnapshot,
  onHistory,
  onModeRequest,
  onSnapshot,
  openRustdesk,
  openSsh,
  refresh,
  saveSettings,
  setAcceptRoutes,
  takeModeRequest,
  type CliError,
  type History,
  type Settings,
  type Snapshot,
} from "./api";
import { backendLabel, clock, connTone } from "./format";
import { usePing } from "./composables/usePing";
import { useToast } from "./composables/useToast";
import ConnectErrorDialog from "./components/ConnectErrorDialog.vue";
import DeviceCard from "./components/DeviceCard.vue";
import ErrorView from "./components/ErrorView.vue";
import DisconnectConfirm from "./components/DisconnectConfirm.vue";
import ModeConfirm from "./components/ModeConfirm.vue";
import ModePanel from "./components/ModePanel.vue";
import RouterPanel from "./components/RouterPanel.vue";
import NetcheckPanel from "./components/NetcheckPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import StatusPill from "./components/ui/StatusPill.vue";
import UiButton from "./components/ui/UiButton.vue";
import UiCallout from "./components/ui/UiCallout.vue";

const tabs = [
  { id: "devices", label: "设备", icon: LayoutGrid },
  { id: "netcheck", label: "网络体检", icon: Stethoscope },
  { id: "settings", label: "设置", icon: Settings2 },
] as const;
const tab = ref<(typeof tabs)[number]["id"]>("devices");

const snapshot = ref<Snapshot | null>(null);
const busy = ref(false);
/** 当前时间，每 5 秒更新：用于“X 分钟前”和测试结果的 30 秒覆盖 */
const now = ref(Date.now());
/** 设备 id → 最近 24 小时的延迟测量 */
const history = ref<History>({});
const { toast, showOk, showError } = useToast();

const overview = computed(() => snapshot.value?.overview ?? null);
const peers = computed(() => overview.value?.peers ?? []);
const onlineCount = computed(() => peers.value.filter((p) => p.conn.kind !== "offline").length);
/** 组网没在运行（已断开、需要登录等）：设备列表是断开前的旧信息 */
const linkDown = computed(() => !!overview.value && overview.value.backendState !== "Running");
const { pings, testing, testOne, testablePeers, testingAll, testAll } = usePing(
  peers,
  () => (now.value = Date.now()),
);

/** 顶部的整体状态：文字 + 状态色 */
const overall = computed(() => {
  const o = overview.value;
  if (!o) return { label: "出错", tone: "error" };
  if (o.backendState !== "Running") return { label: backendLabel(o.backendState), tone: "offline" };
  if (o.controlUnreachable) return { label: "控制端连不上", tone: "error" };
  return { label: "已连接", tone: "direct" };
});

/** 设备页顶部的概览：按连接状态统计台数，打洞中和空闲没有设备时不显示 */
const stats = computed(() => {
  const count = (tone: string) => peers.value.filter((p) => connTone(p.conn.kind) === tone).length;
  return [
    { tone: "direct", label: "直连", n: count("direct"), always: true },
    { tone: "relay", label: "中继", n: count("relay"), always: true },
    { tone: "punching", label: "打洞中", n: count("punching"), always: false },
    { tone: "idle", label: "在线·空闲", n: count("idle"), always: false },
    { tone: "offline", label: "离线", n: count("offline"), always: true },
  ].filter((s) => s.always || s.n > 0);
});

/** 1.102.4-t1a2b3c-g4d5e6f → 1.102.4 */
const shortVersion = (v: string | null) => (v ? v.split("-")[0] : "版本未知");

// ---------- 连接 / 断开组网 ----------
const linkBusy = ref(false);
const showDisconnect = ref(false);
const disconnectError = ref<CliError | null>(null);

function openDisconnect() {
  disconnectError.value = null;
  showDisconnect.value = true;
}

async function doDisconnect() {
  linkBusy.value = true;
  try {
    snapshot.value = await disconnect();
    showDisconnect.value = false;
    showOk("已断开组网");
  } catch (e) {
    disconnectError.value = e as CliError;
  } finally {
    linkBusy.value = false;
  }
}

async function doConnect() {
  linkBusy.value = true;
  connectError.value = null;
  try {
    snapshot.value = await connect();
    showOk("已连接组网");
  } catch (e) {
    // 弹窗显示原因和原始输出，方便排查
    connectError.value = e as CliError;
  } finally {
    linkBusy.value = false;
  }
}

/** 连接失败的详情（弹窗） */
const connectError = ref<CliError | null>(null);

async function doRefresh() {
  if (busy.value) return;
  busy.value = true;
  try {
    snapshot.value = await refresh();
  } finally {
    busy.value = false;
  }
}

async function copyIp(ip: string) {
  try {
    await navigator.clipboard.writeText(ip);
    showOk(`已复制 ${ip}`);
  } catch {
    showError("复制失败");
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
    showOk(`已切换到${on ? "出门模式" : "在家模式"}`);
  } catch (e) {
    if (fromDialog) modeError.value = e as CliError;
    else showError(`切换失败：${(e as CliError).message}`);
  } finally {
    modeBusy.value = false;
  }
}

/** 快捷连接（SSH / RustDesk），失败时提示原因 */
async function launch(open: (arg: string) => Promise<void>, arg: string) {
  try {
    await open(arg);
  } catch (e) {
    showError(`打开失败：${String(e)}`);
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
    showError(`保存失败：${String(e)}`);
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
  unlisteners.push(await onHistory((h) => (history.value = h)));
  history.value = await getHistory();
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
    <!-- 顶栏：左边品牌和状态，右边操作；下面一行是标签页 -->
    <header class="z-10 border-b border-line backdrop-blur-xl" style="background: var(--md-header)">
      <div class="flex items-center gap-3 px-6 pt-3 pb-2">
        <span
          class="grid size-8 shrink-0 place-items-center rounded-ctl text-white"
          style="background: var(--md-grad); box-shadow: 0 6px 16px -6px rgb(112 72 232 / 0.8)"
        >
          <Network class="size-[18px]" />
        </span>
        <h1 class="text-base font-bold tracking-tight">MeshDock</h1>
        <StatusPill v-if="snapshot" :tone="overall.tone" :label="overall.label" />
        <span v-if="overview" class="text-xs whitespace-nowrap text-muted">
          <template v-if="!linkDown"><span class="num text-fg">{{ onlineCount }}</span> 台在线 / </template>共
          <span class="num text-fg">{{ peers.length }}</span> 台
        </span>

        <div class="ml-auto flex items-center gap-2">
          <span v-if="snapshot" class="num mr-1 text-xs text-faint" title="最近一次读取组网状态的时间">
            {{ clock(snapshot.updatedAtMs) }}
          </span>
          <UiButton
            v-if="overview && tab === 'devices'"
            size="sm"
            :icon="Gauge"
            :loading="testingAll"
            :disabled="testingAll || linkDown || testablePeers.length === 0"
            title="测试所有在线设备的实际路径和延迟"
            @click="testAll"
          >
            全部测试
          </UiButton>
          <UiButton size="sm" :disabled="busy" title="立即读取组网状态" @click="doRefresh">
            <RefreshCw class="ui-btn-icon" :class="{ 'animate-spin': busy }" />
            刷新
          </UiButton>
          <!-- 连接 / 断开组网：断开需二次确认，连接直接执行 -->
          <UiButton
            v-if="overview && overview.backendState === 'Running'"
            variant="danger-outline"
            size="sm"
            :icon="Unplug"
            :disabled="linkBusy"
            title="退出组网（会中断经组网连进本机的 SSH、RustDesk）"
            @click="openDisconnect"
          >
            断开
          </UiButton>
          <UiButton
            v-else-if="overview"
            variant="primary"
            size="sm"
            :icon="Plug"
            :loading="linkBusy"
            :disabled="linkBusy"
            title="重新连接组网（不会改动现有设置）"
            @click="doConnect"
          >
            {{ linkBusy ? "正在连接…" : "连接" }}
          </UiButton>
        </div>
      </div>

      <!-- 标签页 -->
      <nav class="flex gap-1 px-4" role="tablist">
        <button
          v-for="t in tabs"
          :key="t.id"
          type="button"
          role="tab"
          :aria-selected="tab === t.id"
          class="relative inline-flex items-center gap-1.5 px-3 pt-1.5 pb-2.5 text-sm transition-colors"
          :class="tab === t.id ? 'font-medium text-fg' : 'text-muted hover:text-fg'"
          @click="tab = t.id"
        >
          <component :is="t.icon" class="size-4" :class="{ 'text-accent': tab === t.id }" />
          {{ t.label }}
          <span
            class="absolute inset-x-2 -bottom-px h-[3px] rounded-full transition-all duration-200"
            :class="tab === t.id ? 'opacity-100' : 'scale-x-50 opacity-0'"
            style="background: var(--md-grad)"
          />
        </button>
      </nav>
    </header>

    <!-- 网络体检（与组网状态无关，出错时也能用） -->
    <main v-show="tab === 'netcheck'" class="md-pane flex-1 overflow-y-auto px-6 py-5">
      <NetcheckPanel />
    </main>

    <main v-show="tab === 'settings'" class="md-pane flex-1 overflow-y-auto px-6 py-5">
      <SettingsPanel :settings="settings" :gateways="overview?.location.gateways ?? []" @save="updateSettings" />
    </main>

    <main v-show="tab === 'devices'" class="md-pane flex-1 overflow-y-auto px-6 py-5">
      <!-- 加载中 -->
      <div v-if="!snapshot" class="flex h-full items-center justify-center gap-2 text-muted">
        <LoaderCircle class="size-5 animate-spin text-accent" />
        正在读取组网状态…
      </div>

      <!-- 出错 -->
      <ErrorView v-else-if="snapshot.error" :error="snapshot.error" :busy="busy" @retry="doRefresh" />

      <template v-else-if="overview">
        <!-- 概览：各连接状态的台数 -->
        <div
          v-if="peers.length"
          class="mb-5 grid grid-cols-[repeat(auto-fit,minmax(120px,1fr))] gap-3 transition-opacity"
          :class="{ 'opacity-50': linkDown }"
        >
          <div v-for="s in stats" :key="s.tone" class="ui-card flex items-center gap-3 px-4 py-3">
            <span class="h-8 w-1 shrink-0 rounded-full" :style="{ background: `var(--md-${s.tone})` }" />
            <div class="min-w-0">
              <div class="num text-2xl leading-none font-semibold" :class="{ 'text-faint': s.n === 0 }">{{ s.n }}</div>
              <div class="mt-1 text-xs text-muted">{{ s.label }}</div>
            </div>
          </div>
        </div>

        <ModePanel
          class="mb-5"
          :overview="overview"
          :settings="settings"
          :busy="modeBusy"
          @request="requestMode"
          @fix-home="switchMode(false, false)"
          @set-at-home="updateAtHome"
        />

        <!-- 组网断开时，下面的路由器和设备是断开前的旧信息：说明并调暗 -->
        <UiCallout v-if="linkDown" tone="offline" :icon="Unplug" class="mb-5">
          组网{{ backendLabel(overview.backendState) }}：下面的子网路由器和设备是断开前最后看到的信息，可能已经过时。点顶栏“连接”重新加入组网后会自动更新。
        </UiCallout>

        <div :class="{ 'pointer-events-none opacity-50 select-none': linkDown }" :aria-disabled="linkDown">
          <RouterPanel v-for="r in overview.routers" :key="r.name" class="mb-5" :router="r" :now="now" />
        </div>

        <!-- 本机 -->
        <section v-if="overview.selfDevice" class="mb-6">
          <h2 class="mb-2.5 flex items-center gap-2 text-xs font-semibold tracking-wide text-muted">
            本机<span class="h-px flex-1 bg-line" />
          </h2>
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
        <section :class="{ 'pointer-events-none opacity-50 select-none': linkDown }" :aria-disabled="linkDown">
          <h2 class="mb-2.5 flex items-center gap-2 text-xs font-semibold tracking-wide text-muted">
            组网设备<span class="num font-normal text-faint">{{ peers.length }}</span><span class="h-px flex-1 bg-line" />
          </h2>
          <div v-if="peers.length === 0" class="ui-card flex flex-col items-center gap-3 py-12 text-center text-muted">
            <Network class="size-8 text-faint" />
            组网里还没有其他设备
          </div>
          <div v-else class="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4">
            <DeviceCard
              v-for="d in peers"
              :key="d.id"
              :device="d"
              :now="now"
              :ping="pings[d.id]"
              :testing="testing.has(d.id)"
              @copy="copyIp"
              :rustdesk="overview.rustdesk"
              :history="history[d.id]"
              @test="testOne(d)"
              @ssh="launch(openSsh, $event)"
              @rustdesk="launch(openRustdesk, $event)"
            />
          </div>
        </section>
      </template>
    </main>

    <ConnectErrorDialog
      v-if="connectError"
      :error="connectError"
      :busy="linkBusy"
      @retry="doConnect"
      @close="connectError = null"
    />

    <DisconnectConfirm
      v-if="showDisconnect"
      :busy="linkBusy"
      :error="disconnectError"
      @confirm="doDisconnect"
      @cancel="showDisconnect = false"
    />

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
        class="fixed bottom-5 left-1/2 z-30 inline-flex -translate-x-1/2 items-center gap-2 rounded-full border border-line bg-surface px-4 py-2 text-sm shadow-raised"
        role="status"
      >
        <span
          class="grid size-5 place-items-center rounded-full"
          :style="
            toast.kind === 'error'
              ? { color: 'var(--md-error)', background: 'var(--md-error-soft)' }
              : { color: 'var(--md-direct)', background: 'var(--md-direct-soft)' }
          "
        >
          <TriangleAlert v-if="toast.kind === 'error'" class="size-3.5" />
          <Check v-else class="size-3.5" />
        </span>
        {{ toast.text }}
      </div>
    </Transition>
  </div>
</template>
