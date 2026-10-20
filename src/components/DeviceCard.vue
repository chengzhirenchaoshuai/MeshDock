<script setup lang="ts">
import { computed } from "vue";
import { ArrowDown, ArrowUp, Copy, Gauge, MonitorSmartphone, Send, SquareTerminal } from "@lucide/vue";
import type { Device, PingRecord, Sample } from "../api";
import LatencyChart from "./LatencyChart.vue";
import { connTone, formatBytes, measureLine, timeAgo } from "../format";
import OsIcon from "./OsIcon.vue";
import SysInfoPanel from "./SysInfoPanel.vue";
import ClientSetupPanel from "./ClientSetupPanel.vue";
import StatusPill from "./ui/StatusPill.vue";
import UiButton from "./ui/UiButton.vue";

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
  /** 最近 24 小时的延迟测量 */
  history?: Sample[];
  /** 正在给这台设备发文件 */
  sending?: boolean;
}>();
const emit = defineEmits<{ copy: [ip: string]; test: []; ssh: [alias: string]; rustdesk: [ip: string]; send: [] }>();

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
/** 当前连接方式对应的状态色名称：胶囊、左侧色条、图标底色共用 */
const tone = computed(() => connTone(conn.value.kind));

/** 测速行：最近一次测量（手动测试或后台测速）的延迟和路径，或失败原因 */
const pingLine = computed(() => measureLine(props.ping, props.history, props.now));

/** 时间信息：离线显示最后在线，在线显示最近握手 */
const timeLine = computed(() => {
  const d = props.device;
  if (offline.value) return { text: `最后在线 ${timeAgo(d.lastSeenMs, props.now)}`, title: undefined };
  if (isSelf.value) return null;
  if (d.lastHandshakeMs) return { text: `最近握手 ${timeAgo(d.lastHandshakeMs, props.now)}`, title: undefined };
  return {
    text: "还没有数据往来",
    title:
      "本机和这台设备之间还没有建立过加密连接，所以没有握手记录、流量为 0。连接方式和测速使用的是 Tailscale 自己的探测包，不需要加密连接；真正访问这台设备（如 SSH、远程桌面）后就会有握手和流量。",
  };
});
</script>

<template>
  <article
    class="ui-card ui-card-interactive flex flex-col gap-3 overflow-hidden p-4 pl-5"
    :class="{ 'opacity-75': offline }"
  >
    <!-- 左侧状态色条 -->
    <span
      class="absolute inset-y-0 left-0 w-[3px] transition-colors duration-300"
      :style="{ background: `var(--md-${tone})` }"
      aria-hidden="true"
    />
    <header class="flex items-start gap-3">
      <div
        class="grid size-10 shrink-0 place-items-center rounded-ctl transition-colors duration-300"
        :style="
          offline
            ? { color: 'var(--md-faint)', background: 'var(--md-subtle)' }
            : { color: `var(--md-${tone})`, background: `var(--md-${tone}-soft)` }
        "
      >
        <OsIcon :os="device.os" />
      </div>
      <div class="min-w-0 flex-1">
        <h3
          class="truncate text-[15px] font-semibold"
          :class="{ 'text-muted': offline }"
          :title="device.dnsName ?? device.name"
        >
          {{ device.name }}
        </h3>
        <button
          v-if="ip"
          type="button"
          class="num group mt-0.5 inline-flex items-center gap-1 rounded text-xs text-muted transition-colors hover:text-accent"
          title="点击复制 IP"
          @click="emit('copy', ip)"
        >
          {{ ip }}
          <Copy class="size-3 opacity-0 transition-opacity group-hover:opacity-100" />
        </button>
      </div>
      <StatusPill :tone="tone" :label="conn.label" :pulse="conn.kind === 'punching'" />
    </header>

    <p v-if="conn.hint" class="text-xs leading-relaxed" :style="{ color: `var(--md-${tone})` }">
      {{ conn.hint }}
    </p>

    <!-- 测速行（本机没有） -->
    <template v-if="!isSelf">
      <div class="flex items-center gap-1.5 rounded-ctl bg-subtle px-2.5 py-1.5 text-xs">
        <Gauge class="size-3.5 shrink-0 text-muted" />
        <span
          class="min-w-0 flex-1 truncate"
          :class="pingLine.failed ? 'text-error' : 'text-muted'"
          :title="pingLine.raw ? `${pingLine.text}

${pingLine.raw}` : pingLine.text"
        >
          {{ testing ? "正在测试…" : pingLine.text }}
        </span>
      </div>

      <!-- 操作按钮：单独一行，有几个就平分宽度；图标用强调色，悬停时染上强调色 -->
      <div class="grid auto-cols-fr grid-flow-col gap-2">
        <UiButton
          v-if="device.sshAlias"
          size="sm"
          class="[&_.ui-btn-icon]:text-accent enabled:hover:bg-[var(--md-accent-soft)] enabled:hover:text-accent"
          :icon="SquareTerminal"
          :disabled="offline"
          :title="offline ? '设备离线' : `在新终端中执行 ssh ${device.sshAlias}`"
          @click="emit('ssh', device.sshAlias)"
        >
          SSH
        </UiButton>
        <UiButton
          v-if="rustdesk && ip"
          size="sm"
          class="[&_.ui-btn-icon]:text-accent enabled:hover:bg-[var(--md-accent-soft)] enabled:hover:text-accent"
          :icon="MonitorSmartphone"
          :disabled="offline"
          :title="offline ? '设备离线' : `用 RustDesk 直连 ${ip}（对方需在 RustDesk 中允许 IP 直接访问）`"
          @click="emit('rustdesk', ip)"
        >
          RustDesk
        </UiButton>
        <UiButton
          size="sm"
          class="[&_.ui-btn-icon]:text-accent enabled:hover:bg-[var(--md-accent-soft)] enabled:hover:text-accent"
          :icon="Send"
          :loading="sending"
          :disabled="sending || offline"
          :title="offline ? '设备离线' : sending ? '正在发送…' : '选择文件，用 Taildrop 发给这台设备（对方的官方客户端会存到“下载”文件夹）'"
          @click="emit('send')"
        >
          发文件
        </UiButton>
        <UiButton
          size="sm"
          class="[&_.ui-btn-icon]:text-accent enabled:hover:bg-[var(--md-accent-soft)] enabled:hover:text-accent"
          :icon="Gauge"
          :loading="testing"
          :disabled="testing || offline"
          :title="offline ? '设备离线，无法测试' : '测试实际路径和延迟'"
          @click="emit('test')"
        >
          测试
        </UiButton>
      </div>

      <!-- 系统信息：只对能 ssh 的 Linux 设备采集 -->
      <SysInfoPanel
        v-if="device.sshAlias && device.os === 'linux'"
        :alias="device.sshAlias"
        :device-id="device.id"
        :now="now"
        :offline="offline"
      />
      <!-- 官方客户端的无人值守与托盘自启：只对能 ssh 的 Windows 设备 -->
      <ClientSetupPanel
        v-if="device.sshAlias && device.os === 'windows'"
        :alias="device.sshAlias"
        :name="device.name"
        :offline="offline"
      />
    </template>

    <!-- 24 小时延迟历史（本机没有） -->
    <template v-if="!isSelf">
      <LatencyChart v-if="history?.length" :samples="history" :now="now" />
      <p v-else class="text-[11px] text-faint">还没有延迟历史，程序运行时会按设置的间隔自动测速</p>
    </template>

    <footer class="mt-auto flex items-center gap-4 border-t border-line pt-3 text-xs text-muted">
      <span v-if="note" class="min-w-0 leading-relaxed">{{ note }}</span>
      <template v-else>
        <span class="num inline-flex items-center gap-1" title="从这台设备收到的数据">
          <ArrowDown class="size-3.5 text-faint" />{{ formatBytes(device.rxBytes) }}
        </span>
        <span class="num inline-flex items-center gap-1" title="发给这台设备的数据">
          <ArrowUp class="size-3.5 text-faint" />{{ formatBytes(device.txBytes) }}
        </span>
      </template>
      <span v-if="timeLine" class="ml-auto truncate" :title="timeLine.title">{{ timeLine.text }}</span>
    </footer>
  </article>
</template>
