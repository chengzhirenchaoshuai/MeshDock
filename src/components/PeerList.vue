<script setup lang="ts">
import { computed, ref } from "vue";
import { Network, Search } from "@lucide/vue";
import type { Device, History, PingRecord } from "../api";
import { filterPeers, isFiltering, type PeerFilter } from "../devices";
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
}>();
const emit = defineEmits<{
  copy: [ip: string];
  test: [device: Device];
  ssh: [alias: string];
  rustdesk: [ip: string];
  send: [id: string];
}>();

const filter = ref<PeerFilter>({ query: "", onlineOnly: false });
const shown = computed(() => filterPeers(props.peers, filter.value));
const filtering = computed(() => isFiltering(filter.value));

function clearFilter() {
  filter.value = { query: "", onlineOnly: false };
}
</script>

<template>
  <section :class="{ 'pointer-events-none opacity-50 select-none': disabled }" :aria-disabled="disabled">
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
            placeholder="搜索名称、IP、系统"
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
        @copy="emit('copy', $event)"
        @test="emit('test', d)"
        @ssh="emit('ssh', $event)"
        @rustdesk="emit('rustdesk', $event)"
        @send="emit('send', d.id)"
      />
    </div>
  </section>
</template>
