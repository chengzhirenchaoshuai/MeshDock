<script setup lang="ts">
import { computed } from "vue";
import { ArrowDown, ArrowUp, Copy } from "@lucide/vue";
import type { Device } from "../api";
import { connTone, formatBytes, timeAgo } from "../format";
import ConnBadge from "./ConnBadge.vue";
import OsIcon from "./OsIcon.vue";

/** note：替代流量显示的说明文字（本机卡片用来显示客户端版本） */
const props = defineProps<{ device: Device; now: number; note?: string }>();
const emit = defineEmits<{ copy: [ip: string] }>();

const offline = computed(() => props.device.conn.kind === "offline");
const isSelf = computed(() => props.device.conn.kind === "self-node");
/** 组网 IPv4 放在前面显示 */
const ip = computed(() => props.device.ips.find((a) => !a.includes(":")) ?? props.device.ips[0]);
const hintColor = computed(() => `var(--md-${connTone(props.device.conn.kind)})`);

/** 时间信息：离线显示最后在线，在线显示最近握手 */
const timeLine = computed(() => {
  const d = props.device;
  if (offline.value) return `最后在线 ${timeAgo(d.lastSeenMs, props.now)}`;
  if (isSelf.value) return null;
  return d.lastHandshakeMs ? `最近握手 ${timeAgo(d.lastHandshakeMs, props.now)}` : "还没有握手过";
});
</script>

<template>
  <article
    class="flex flex-col gap-3 rounded-card border border-line bg-surface p-4 shadow-card transition-opacity"
    :class="{ 'opacity-70': offline }"
  >
    <header class="flex items-start gap-3">
      <div
        class="grid size-9 shrink-0 place-items-center rounded-ctl bg-subtle"
        :class="offline ? 'text-faint' : 'text-muted'"
      >
        <OsIcon :os="device.os" />
      </div>
      <div class="min-w-0 flex-1">
        <h3 class="truncate text-[15px] font-semibold" :title="device.dnsName ?? device.name">
          {{ device.name }}
        </h3>
        <button
          v-if="ip"
          class="num group mt-0.5 inline-flex items-center gap-1 rounded text-xs text-muted hover:text-accent"
          title="点击复制 IP"
          @click="emit('copy', ip)"
        >
          {{ ip }}
          <Copy class="size-3 opacity-0 transition-opacity group-hover:opacity-100" />
        </button>
      </div>
      <ConnBadge :conn="device.conn" />
    </header>

    <p v-if="device.conn.hint" class="text-xs leading-relaxed" :style="{ color: hintColor }">
      {{ device.conn.hint }}
    </p>

    <footer class="mt-auto flex items-center gap-4 border-t border-line pt-3 text-xs text-muted">
      <span v-if="note">{{ note }}</span>
      <template v-else>
        <span class="num inline-flex items-center gap-1" title="从这台设备收到的数据">
          <ArrowDown class="size-3.5" />{{ formatBytes(device.rxBytes) }}
        </span>
        <span class="num inline-flex items-center gap-1" title="发给这台设备的数据">
          <ArrowUp class="size-3.5" />{{ formatBytes(device.txBytes) }}
        </span>
      </template>
      <span v-if="timeLine" class="ml-auto truncate">{{ timeLine }}</span>
    </footer>
  </article>
</template>
