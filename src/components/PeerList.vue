<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Network, Search } from "@lucide/vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Device, History, PingRecord } from "../api";
import { arrangePeers, isFiltering, type PeerFilter, type PeerMarks } from "../devices";
import DeviceCard from "./DeviceCard.vue";

/** 组网设备列表：标题栏带搜索和“只看在线”，下面是设备卡片 */
const props = defineProps<{
  peers: Device[];
  now: number;
  pings: Record<string, PingRecord>;
  testing: Set<string>;
  history: History;
  /** 本机是否装了 RustDesk */
  rustdesk: boolean;
  /** 正在发文件的设备 id */
  sending: Set<string>;
  /** 组网断开：列表是断开前的旧信息，调暗并禁止操作 */
  disabled: boolean;
  /** 本机给设备加的备注和收藏 */
  marks: PeerMarks;
}>();
const emit = defineEmits<{
  copy: [ip: string];
  test: [device: Device];
  ssh: [alias: string];
  rustdesk: [ip: string];
  send: [id: string];
  /** 文件拖到了这台设备的卡片上 */
  drop: [id: string];
  /** 文件拖到了列表里，但没落在在线设备的卡片上 */
  dropMiss: [];
  /** 备注或收藏改了：带回新的备注和收藏 */
  marks: [marks: PeerMarks];
}>();

const filter = ref<PeerFilter>({ query: "", onlineOnly: false });
const shown = computed(() => arrangePeers(props.peers, filter.value, props.marks));

function setFavorite(id: string, on: boolean) {
  const rest = props.marks.favorites.filter((f) => f !== id);
  emit("marks", { ...props.marks, favorites: on ? [...rest, id] : rest });
}

function setRemark(id: string, text: string) {
  const notes = { ...props.marks.notes };
  if (text) notes[id] = text;
  else delete notes[id];
  emit("marks", { ...props.marks, notes });
}
const filtering = computed(() => isFiltering(filter.value));

function clearFilter() {
  filter.value = { query: "", onlineOnly: false };
}

// ---------- 把文件拖到设备卡片上发送 ----------
const root = ref<HTMLElement | null>(null);
/** 拖动时光标下的设备 id，卡片高亮 */
const dropId = ref<string | null>(null);

/**
 * 拖放坐标（物理像素）下的设备卡片：不在列表里（如在其他标签页、组网断开）返回 undefined，
 * 在列表里但不在可接收的卡片上返回 null
 */
function cardAt(pos: { x: number; y: number }): string | null | undefined {
  const ratio = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(pos.x / ratio, pos.y / ratio);
  if (props.disabled || !el || !root.value?.contains(el)) return undefined;
  return el.closest<HTMLElement>("[data-drop-id]")?.dataset.dropId ?? null;
}

let unlisten: (() => void) | undefined;
onMounted(async () => {
  unlisten = await getCurrentWebview().onDragDropEvent(({ payload: p }) => {
    if (p.type === "enter" || p.type === "over") {
      dropId.value = cardAt(p.position) ?? null;
    } else if (p.type === "drop") {
      dropId.value = null;
      const id = cardAt(p.position);
      if (id) emit("drop", id);
      else if (id === null) emit("dropMiss");
    } else {
      dropId.value = null;
    }
  });
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <section
    ref="root"
    :class="{ 'pointer-events-none opacity-50 select-none': disabled }"
    :aria-disabled="disabled"
  >
    <h2 class="mb-2.5 flex items-center gap-2 text-xs font-semibold tracking-wide text-muted">
      组网设备<span class="num font-normal text-faint">
        <template v-if="filtering">{{ shown.length }} / </template>{{ peers.length }}
      </span>
      <span class="h-px flex-1 bg-line" />
      <!-- 搜索和筛选：设备不多时也显示，方便按 IP 找设备 -->
      <template v-if="peers.length">
        <label class="flex cursor-pointer items-center gap-1.5 font-normal tracking-normal">
          <input v-model="filter.onlineOnly" type="checkbox" class="accent-[var(--md-accent)]" />只看在线
        </label>
        <span class="relative font-normal tracking-normal">
          <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-faint" />
          <input
            v-model="filter.query"
            type="search"
            class="ui-input w-48 py-1 pl-7 text-xs select-text"
            placeholder="搜索名称、备注、IP、系统"
            @keydown.esc="filter.query = ''"
          />
        </span>
      </template>
    </h2>
    <div v-if="peers.length === 0" class="ui-card flex flex-col items-center gap-3 py-12 text-center text-muted">
      <Network class="size-8 text-faint" />
      组网里还没有其他设备
    </div>
    <div v-else-if="shown.length === 0" class="ui-card py-8 text-center text-sm text-muted">
      没有符合条件的设备
      <button type="button" class="ml-2 text-accent hover:underline" @click="clearFilter">清除筛选</button>
    </div>
    <div v-else class="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4">
      <DeviceCard
        v-for="d in shown"
        :key="d.id"
        :device="d"
        :now="now"
        :ping="pings[d.id]"
        :testing="testing.has(d.id)"
        :rustdesk="rustdesk"
        :history="history[d.id]"
        :sending="sending.has(d.id)"
        :data-drop-id="d.conn.kind === 'offline' || sending.has(d.id) ? undefined : d.id"
        :drop-target="dropId === d.id"
        :favorite="marks.favorites.includes(d.id)"
        :remark="marks.notes[d.id]"
        @favorite="setFavorite(d.id, $event)"
        @remark="setRemark(d.id, $event)"
        @copy="emit('copy', $event)"
        @test="emit('test', d)"
        @ssh="emit('ssh', $event)"
        @rustdesk="emit('rustdesk', $event)"
        @send="emit('send', d.id)"
      />
    </div>
  </section>
</template>
