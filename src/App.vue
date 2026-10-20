<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  Check,
  Gauge,
  LayoutGrid,
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
  refresh,
  saveSettings,
  setAcceptRoutes,
  setExitNode,
  setPrefSwitch,
  takeModeRequest,
  type CliError,
  type History,
  type PrefSwitch,
  type Settings,
  type Snapshot,
} from "./api";
import { backendLabel, clock } from "./format";
import { applyPalette } from "./palette";
import { usePing } from "./composables/usePing";
import { useToast } from "./composables/useToast";
import ConnectErrorDialog from "./components/ConnectErrorDialog.vue";
import DevicesView from "./components/DevicesView.vue";
import DisconnectConfirm from "./components/DisconnectConfirm.vue";
import ModeConfirm from "./components/ModeConfirm.vue";
import NetcheckPanel from "./components/NetcheckPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import StatusPill from "./components/ui/StatusPill.vue";
import UiButton from "./components/ui/UiButton.vue";

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

// ---------- 在家 / 出门模式 ----------
const settings = ref<Settings | null>(null);
// 配色方案随设置切换（读回设置、在设置页修改时）
watch(
  () => settings.value?.palette,
  (p) => p && applyPalette(p),
);
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

// ---------- 出口节点和其他本机组网选项 ----------
const prefBusy = ref(false);

/** 执行一项本机设置的修改，成功提示 ok，失败用提示条显示原因 */
async function applyPref(run: () => Promise<Snapshot>, ok: string) {
  if (prefBusy.value) return;
  prefBusy.value = true;
  try {
    snapshot.value = await run();
    showOk(ok);
  } catch (e) {
    showError(`设置失败：${(e as CliError).message}`);
  } finally {
    prefBusy.value = false;
  }
}

function selectExit(id: string | null) {
  const name = peers.value.find((d) => d.id === id)?.name;
  applyPref(() => setExitNode(id), id ? `已改为经 ${name} 上网` : "已停用出口");
}

function togglePref(pref: PrefSwitch, on: boolean, ok: string) {
  applyPref(() => setPrefSwitch(pref, on), ok);
}

/** 读不到网关时的手动标记 */
function updateAtHome(atHome: true | null) {
  updateSettings({ atHome });
}

/** 改设置：先合并到本地最新值再整体保存，连续快速修改时后一次不会拿旧值覆盖前一次 */
async function updateSettings(change: Partial<Settings>) {
  if (!settings.value) return;
  const next = { ...settings.value, ...change };
  settings.value = next;
  try {
    const saved = await saveSettings(next);
    // 保存期间又有新的修改时，以本地更新的为准
    if (settings.value === next) settings.value = saved;
  } catch (e) {
    showError(`保存失败：${String(e)}`);
    settings.value = await getSettings().catch(() => settings.value);
  }
}

/** 托盘点了模式菜单：切到设备页并弹出确认框 */
async function takeTrayRequest() {
  const on = await takeModeRequest();
  if (on === null) return;
  tab.value = "devices";
  requestMode(on);
}

// 启动后组网一连上就测一遍全部设备（结果也记入延迟历史，后台测速从下一个间隔开始）
const stopStartupTest = watch(
  () => !linkDown.value && testablePeers.value.length > 0,
  (ready) => {
    if (!ready) return;
    stopStartupTest();
    testAll();
  },
);

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
          class="grid size-8 shrink-0 place-items-center rounded-ctl text-[var(--md-grad-fg)]"
          style="background: var(--md-grad); box-shadow: 0 6px 16px -6px color-mix(in srgb, var(--md-grad-shadow) 80%, transparent)"
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
          <!-- MeshDock 有新版本：设置标签上加小圆点 -->
          <span
            v-if="t.id === 'settings' && overview?.appUpdate.state === 'available'"
            class="size-1.5 rounded-full bg-accent"
            title="MeshDock 有新版本"
          />
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
      <SettingsPanel
        :settings="settings"
        :gateways="overview?.location.gateways ?? []"
        :prefs="overview?.prefs ?? null"
        :pref-busy="prefBusy"
        :app-update="overview?.appUpdate ?? null"
        @save="updateSettings"
        @toggle-pref="togglePref"
        @switched="(s, name) => ((snapshot = s), showOk(`已切换到 ${name}`))"
      />
    </main>

    <main v-show="tab === 'devices'" class="md-pane flex-1 overflow-y-auto px-6 py-5">
      <DevicesView
        :snapshot="snapshot"
        :settings="settings"
        :now="now"
        :history="history"
        :pings="pings"
        :testing="testing"
        :busy="busy"
        :mode-busy="modeBusy"
        :pref-busy="prefBusy"
        @retry="doRefresh"
        @request-mode="requestMode"
        @fix-home="switchMode(false, false)"
        @set-at-home="updateAtHome"
        @select-exit="selectExit"
        @toggle-pref="togglePref"
        @test="testOne"
        @ok="showOk"
        @error="showError"
        @save="updateSettings"
      />
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
