<script setup lang="ts">
import { computed, ref } from "vue";
import { LoaderCircle, Unplug } from "@lucide/vue";
import {
  openRustdesk,
  openSsh,
  sendDropped,
  sendFiles,
  type CliError,
  type SendResult,
  type Device,
  type History,
  type PingRecord,
  type PrefSwitch,
  type Settings,
  type Snapshot,
} from "../api";
import { peerStats, type PeerMarks } from "../devices";
import { backendLabel } from "../format";
import DeviceCard from "./DeviceCard.vue";
import ErrorView from "./ErrorView.vue";
import ExitNodePanel from "./ExitNodePanel.vue";
import ModePanel from "./ModePanel.vue";
import PeerList from "./PeerList.vue";
import RouterPanel from "./RouterPanel.vue";
import ServePanel from "./ServePanel.vue";
import UiCallout from "./ui/UiCallout.vue";

/** 设备页：概览、在家 / 出门模式、出口、子网路由器、本机和组网设备 */
const props = defineProps<{
  snapshot: Snapshot | null;
  settings: Settings | null;
  now: number;
  history: History;
  pings: Record<string, PingRecord>;
  testing: Set<string>;
  /** 正在读取组网状态（出错页的“重试”按钮用） */
  busy: boolean;
  modeBusy: boolean;
  prefBusy: boolean;
}>();
const emit = defineEmits<{
  retry: [];
  requestMode: [on: boolean];
  fixHome: [];
  setAtHome: [atHome: true | null];
  selectExit: [id: string | null];
  togglePref: [pref: PrefSwitch, on: boolean, ok: string];
  test: [device: Device];
  ok: [text: string];
  error: [text: string];
  /** 保存设置（备注、收藏改了） */
  save: [settings: Settings];
}>();

const overview = computed(() => props.snapshot?.overview ?? null);
const peers = computed(() => overview.value?.peers ?? []);
/** 组网没在运行（已断开、需要登录等）：设备列表是断开前的旧信息 */
const linkDown = computed(() => !!overview.value && overview.value.backendState !== "Running");
const stats = computed(() => peerStats(peers.value));

/** 备注和收藏存在设置里 */
const marks = computed<PeerMarks>(() => ({
  notes: props.settings?.deviceNotes ?? {},
  favorites: props.settings?.favorites ?? [],
}));

function saveMarks(m: PeerMarks) {
  if (props.settings) emit("save", { ...props.settings, deviceNotes: m.notes, favorites: m.favorites });
}

/** 1.102.4-t1a2b3c-g4d5e6f → 1.102.4 */
const shortVersion = (v: string | null) => (v ? v.split("-")[0] : "版本未知");

async function copyIp(ip: string) {
  try {
    await navigator.clipboard.writeText(ip);
    emit("ok", `已复制 ${ip}`);
  } catch {
    emit("error", "复制失败");
  }
}

/** 快捷连接（SSH / RustDesk），失败时提示原因 */
async function launch(open: (arg: string) => Promise<void>, arg: string) {
  try {
    await open(arg);
  } catch (e) {
    emit("error", `打开失败：${String(e)}`);
  }
}

// ---------- 发送文件（Taildrop） ----------
/** 正在发送的设备 id */
const sending = ref(new Set<string>());

/** 发送文件：send 为 sendFiles（选文件）或 sendDropped（拖到卡片上的文件），取消选择时返回 null */
async function sendTo(id: string, send: (id: string) => Promise<SendResult | null> = sendFiles) {
  if (sending.value.has(id)) return;
  sending.value = new Set(sending.value).add(id);
  try {
    const r = await send(id);
    if (r) {
      const what = r.files.length === 1 ? r.files[0] : `${r.files.length} 个文件`;
      emit("ok", `已把 ${what} 发给 ${r.device}`);
    }
  } catch (e) {
    emit("error", (e as CliError).message);
  } finally {
    const next = new Set(sending.value);
    next.delete(id);
    sending.value = next;
  }
}
</script>

<template>
  <!-- 加载中 -->
  <div v-if="!snapshot" class="flex h-full items-center justify-center gap-2 text-muted">
    <LoaderCircle class="size-5 animate-spin text-accent" />
    正在读取组网状态…
  </div>

  <!-- 出错 -->
  <ErrorView v-else-if="snapshot.error" :error="snapshot.error" :busy="busy" @retry="emit('retry')" />

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
      @request="emit('requestMode', $event)"
      @fix-home="emit('fixHome')"
      @set-at-home="emit('setAtHome', $event)"
    />

    <ExitNodePanel
      class="mb-5"
      :overview="overview"
      :busy="prefBusy"
      :disabled="linkDown"
      @select="emit('selectExit', $event)"
      @allow-lan="
        emit('togglePref', 'exit-node-allow-lan', $event, `已${$event ? '开启' : '关闭'}“走出口时访问本地局域网”`)
      "
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
          :note="
            `Tailscale 官方客户端 ${shortVersion(overview.version)}` +
            (overview.clientUpdate ? ` · 有新版 ${overview.clientUpdate}，可在官方客户端托盘菜单中更新` : '')
          "
          @copy="copyIp"
        />
        <ServePanel
          :self-ip="overview.selfDevice.ips.find((a) => !a.includes(':')) ?? null"
          :disabled="linkDown"
          @ok="emit('ok', $event)"
          @error="emit('error', $event)"
          @copy="copyIp"
        />
      </div>
    </section>

    <!-- 其他设备 -->
    <PeerList
      :peers="peers"
      :now="now"
      :pings="pings"
      :testing="testing"
      :history="history"
      :rustdesk="overview.rustdesk"
      :sending="sending"
      :disabled="linkDown"
      :marks="marks"
      @marks="saveMarks"
      @copy="copyIp"
      @test="emit('test', $event)"
      @ssh="launch(openSsh, $event)"
      @rustdesk="launch(openRustdesk, $event)"
      @send="sendTo($event)"
      @drop="sendTo($event, sendDropped)"
      @drop-miss="emit('error', '把文件拖到在线设备的卡片上才能发送')"
    />
  </template>
</template>
