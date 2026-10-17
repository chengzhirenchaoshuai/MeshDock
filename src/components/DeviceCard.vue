<script setup lang="ts">
import { computed } from "vue";
import { ArrowDown, ArrowUp, Copy, Gauge, LoaderCircle } from "@lucide/vue";
import type { Device, PingRecord } from "../api";
import { clock, connTone, formatBytes, formatLatency, timeAgo } from "../format";
import ConnBadge from "./ConnBadge.vue";
import OsIcon from "./OsIcon.vue";

/** 测试结果覆盖状态徽标的时长 */
const PING_OVERRIDE_MS = 30_000;

/**
 * note：替代流量显示的说明文字（本机卡片用来显示客户端版本）
 * ping：最近一次测试结果；testing：正在测试；rustdesk：本机是否装了 RustDesk
 */
const props = defineProps<{
  device: Device;
  now: number;
  note?: string;
  ping?: PingRecord;
  testing?: boolean;
  rustdesk?: boolean;
}>();
const emit = defineEmits<{ copy: [ip: string]; test: []; ssh: [alias: string]; rustdesk: [ip: string] }>();

const offline = computed(() => props.device.conn.kind === "offline");
const isSelf = computed(() => props.device.conn.kind === "self-node");
/** 组网 IPv4 放在前面显示 */
const ip = computed(() => props.device.ips.find((a) => !a.includes(":")) ?? props.device.ips[0]);

/** 刚测过时以实测路径为准，30 秒后回到 status 的判断 */
const conn = computed(() => {
  const p = props.ping;
  if (p?.result?.conn && props.now - p.atMs < PING_OVERRIDE_MS) return p.result.conn;
  return props.device.conn;
});
const hintColor = computed(() => `var(--md-${connTone(conn.value.kind)})`);

/** 测速行：延迟和实测路径，或失败原因 */
const pingLine = computed(() => {
  const p = props.ping;
  if (!p) return { text: "延迟未测", failed: false, raw: "" };
  const at = clock(p.atMs);
  if (p.error) return { text: `测试出错：${p.error.message}（${at}）`, failed: true, raw: p.error.raw };
  const r = p.result!;
  if (!r.conn || r.latencyMs == null) {
    return { text: `${r.error ?? "测试失败"}（${at}）`, failed: true, raw: r.raw };
  }
  return { text: `延迟 ${formatLatency(r.latencyMs)} · ${r.conn.label} · ${at} 实测`, failed: false, raw: r.raw };
});

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
      <ConnBadge :conn="conn" />
    </header>

    <p v-if="conn.hint" class="text-xs leading-relaxed" :style="{ color: hintColor }">
      {{ conn.hint }}
    </p>

    <!-- 测速行（本机没有） -->
    <div v-if="!isSelf" class="flex items-center gap-2 text-xs">
      <Gauge class="size-3.5 shrink-0 text-muted" />
      <span
        class="min-w-0 flex-1 truncate"
        :class="pingLine.failed ? 'text-error' : 'text-muted'"
        :title="pingLine.raw || undefined"
      >
        {{ testing ? "正在测试…" : pingLine.text }}
      </span>
      <button
        v-if="device.sshAlias"
        class="shrink-0 rounded-ctl border border-line px-2.5 py-0.5 hover:bg-subtle disabled:cursor-not-allowed disabled:opacity-50"
        :disabled="offline"
        :title="offline ? '设备离线' : `在新终端中执行 ssh ${device.sshAlias}`"
        @click="emit('ssh', device.sshAlias)"
      >
        SSH
      </button>
      <button
        v-if="rustdesk && ip"
        class="shrink-0 rounded-ctl border border-line px-2.5 py-0.5 hover:bg-subtle disabled:cursor-not-allowed disabled:opacity-50"
        :disabled="offline"
        :title="offline ? '设备离线' : `用 RustDesk 直连 ${ip}（对方需在 RustDesk 中允许 IP 直接访问）`"
        @click="emit('rustdesk', ip)"
      >
        RustDesk
      </button>
      <button
        class="inline-flex shrink-0 items-center gap-1 rounded-ctl border border-line px-2.5 py-0.5 hover:bg-subtle disabled:cursor-not-allowed disabled:opacity-50"
        :disabled="testing || offline"
        :title="offline ? '设备离线，无法测试' : '测试实际路径和延迟'"
        @click="emit('test')"
      >
        <LoaderCircle v-if="testing" class="size-3.5 animate-spin" />
        测试
      </button>
    </div>

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
