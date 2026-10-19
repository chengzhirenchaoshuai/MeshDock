<script setup lang="ts">
import { computed } from "vue";
import { CircleCheck, Globe, LoaderCircle, Wifi, WifiOff } from "@lucide/vue";
import type { Overview } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiCallout from "./ui/UiCallout.vue";
import UiCard from "./ui/UiCard.vue";
import UiSwitch from "./ui/UiSwitch.vue";

const props = defineProps<{ overview: Overview; busy: boolean; disabled: boolean }>();
const emit = defineEmits<{
  /** 选用出口：传设备 id，null 表示停用 */
  select: [id: string | null];
  /** 走出口时是否仍可访问本地局域网 */
  allowLan: [on: boolean];
}>();

const prefs = computed(() => props.overview.prefs);
const current = computed(() => prefs.value?.exitNodeId ?? null);
/** 可当出口的设备 */
const candidates = computed(() => props.overview.peers.filter((d) => d.exitOption));
/** 已选的出口设备；选了但不在列表里时为 undefined */
const chosen = computed(() => props.overview.peers.find((d) => d.id === current.value));
/** 已选的出口离线或已不在组网中：本机可能上不了网 */
const chosenDown = computed(() => current.value !== null && (!chosen.value || chosen.value.conn.kind === "offline"));

const options = computed(() => [
  { id: null, name: "不用出口", desc: "直接用本机的网络上网", online: true },
  ...candidates.value.map((d) => ({
    id: d.id as string | null,
    name: d.name,
    desc: d.conn.kind === "offline" ? "离线" : `上网流量经它转发 · ${d.conn.label}`,
    online: d.conn.kind !== "offline",
  })),
]);

/** 都不能点：读不到设置、操作中、组网断开 */
const locked = computed(() => props.busy || props.disabled || !prefs.value);
</script>

<template>
  <UiCard title="出口节点" :icon="Globe">
    <template #actions>
      <span class="ml-auto truncate text-xs text-muted">
        <template v-if="!prefs">读取不到当前设置</template>
        <template v-else-if="current === null">未使用</template>
        <template v-else>正在经 {{ chosen?.name ?? "未知设备" }} 上网</template>
      </span>
      <LoaderCircle v-if="busy" class="size-4 animate-spin text-muted" />
    </template>

    <!-- 组网里没有设备提供出口，也没选过出口：只给一行说明 -->
    <p v-if="candidates.length === 0 && current === null" class="text-xs leading-relaxed text-muted">
      组网里还没有设备提供出口。在要当出口的设备上执行
      <code class="num rounded bg-subtle px-1">tailscale set --advertise-exit-node</code>，
      再在 Headscale 上批准它的 0.0.0.0/0 路由，这里就能选用。
    </p>

    <div v-else class="flex flex-col gap-3">
      <UiCallout v-if="chosenDown" tone="error">
        选用的出口 {{ chosen?.name ?? "设备" }}{{ chosen ? "离线了" : "已不在组网中" }}：本机的上网流量都经它转发，现在可能上不了网。
        <template #action>
          <UiButton variant="danger" size="sm" :disabled="locked" @click="emit('select', null)">停用出口</UiButton>
        </template>
      </UiCallout>

      <div class="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-2" role="radiogroup">
        <button
          v-for="o in options"
          :key="o.id ?? '-'"
          type="button"
          role="radio"
          :aria-checked="current === o.id"
          class="flex items-start gap-2.5 rounded-ctl border p-3 text-left transition-colors disabled:cursor-default"
          :class="
            current === o.id
              ? 'border-transparent'
              : 'border-line bg-surface hover:border-[color-mix(in_srgb,var(--md-accent)_40%,var(--md-line))] disabled:opacity-60'
          "
          :style="
            current === o.id
              ? {
                  background: 'linear-gradient(var(--md-surface), var(--md-surface)) padding-box, var(--md-grad) border-box',
                  boxShadow: '0 0 0 3px var(--md-accent-soft)',
                }
              : undefined
          "
          :disabled="locked || current === o.id || !o.online"
          :title="current === o.id ? '当前就是这个' : !o.online ? '设备离线，不能选用' : o.id ? `经 ${o.name} 上网` : '停用出口'"
          @click="emit('select', o.id)"
        >
          <component
            :is="o.online ? Wifi : WifiOff"
            class="mt-0.5 size-4 shrink-0"
            :class="current === o.id ? 'text-accent' : 'text-muted'"
          />
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-1.5 text-sm font-medium">
              <span class="truncate">{{ o.name }}</span>
              <CircleCheck v-if="current === o.id" class="size-3.5 shrink-0 text-accent" />
            </span>
            <span class="mt-0.5 block truncate text-xs text-muted" :title="o.desc">{{ o.desc }}</span>
          </span>
        </button>
      </div>

      <label class="flex items-center gap-3 rounded-ctl bg-subtle px-3 py-2 text-xs text-muted">
        <span class="min-w-0 flex-1">
          走出口时仍可访问本地局域网（家里或公司的打印机、NAS、路由器管理页）
        </span>
        <UiSwitch
          :model-value="prefs?.exitNodeAllowLan === true"
          :disabled="locked || prefs?.exitNodeAllowLan == null"
          @update:model-value="emit('allowLan', $event)"
        />
      </label>
    </div>
  </UiCard>
</template>
