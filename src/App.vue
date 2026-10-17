<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Check, LoaderCircle, Network, RefreshCw } from "@lucide/vue";
import { getSnapshot, onSnapshot, refresh, type Snapshot } from "./api";
import { backendLabel, clock } from "./format";
import DeviceCard from "./components/DeviceCard.vue";
import ErrorView from "./components/ErrorView.vue";

const snapshot = ref<Snapshot | null>(null);
const busy = ref(false);
/** 用于“X 分钟前”的当前时间，每 30 秒更新一次 */
const now = ref(Date.now());
const toast = ref<string | null>(null);

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

let toastTimer: number | undefined;
async function copyIp(ip: string) {
  try {
    await navigator.clipboard.writeText(ip);
    toast.value = `已复制 ${ip}`;
  } catch {
    toast.value = "复制失败";
  }
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = null), 1800);
}

let unlisten: (() => void) | undefined;
let clockTimer: number | undefined;
onMounted(async () => {
  unlisten = await onSnapshot((s) => (snapshot.value = s));
  clockTimer = window.setInterval(() => (now.value = Date.now()), 30_000);
  snapshot.value = await getSnapshot();
});
onUnmounted(() => {
  unlisten?.();
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
        class="inline-flex items-center gap-1.5 rounded-ctl border border-line px-3 py-1 text-sm hover:bg-subtle disabled:opacity-60"
        :class="{ 'ml-auto': !snapshot }"
        :disabled="busy"
        @click="doRefresh"
      >
        <RefreshCw class="size-4" :class="{ 'animate-spin': busy }" />
        刷新
      </button>
    </header>

    <main class="flex-1 overflow-y-auto px-6 py-5">
      <!-- 加载中 -->
      <div v-if="!snapshot" class="flex h-full items-center justify-center gap-2 text-muted">
        <LoaderCircle class="size-5 animate-spin" />
        正在读取组网状态…
      </div>

      <!-- 出错 -->
      <ErrorView v-else-if="snapshot.error" :error="snapshot.error" :busy="busy" @retry="doRefresh" />

      <template v-else-if="overview">
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
            <DeviceCard v-for="d in peers" :key="d.id" :device="d" :now="now" @copy="copyIp" />
          </div>
        </section>
      </template>
    </main>

    <!-- 复制提示 -->
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
        <Check class="size-4" />
        {{ toast }}
      </div>
    </Transition>
  </div>
</template>
