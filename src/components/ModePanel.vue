<script setup lang="ts">
import { computed } from "vue";
import { CircleCheck, House, LoaderCircle, MapPin, Plane, Route } from "@lucide/vue";
import type { Overview, Settings } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiCallout from "./ui/UiCallout.vue";
import UiCard from "./ui/UiCard.vue";
import UiSwitch from "./ui/UiSwitch.vue";

const props = defineProps<{ overview: Overview; settings: Settings | null; busy: boolean }>();
const emit = defineEmits<{
  /** 请求切换：true 出门、false 在家（由 App 弹确认框） */
  request: [on: boolean];
  /** 一键改回在家模式，不再确认 */
  fixHome: [];
  /** 勾选为 true，取消勾选为 null（没标记），不代表“在外面” */
  setAtHome: [atHome: true | null];
}>();

const mode = computed(() => props.overview.acceptRoutes);
const location = computed(() => props.overview.location);
const atHome = computed(() => location.value.atHome);

/** 位置说明：自动识别时写明依据的网关 */
const locationText = computed(() => {
  const l = location.value;
  if (!l.auto) return null;
  return l.atHome
    ? `在家（默认网关 ${l.homeGateway}）`
    : `在外面（默认网关 ${l.gateways.join("、")}，不是家里的 ${l.homeGateway}）`;
});

/** 子网路由说明，例如“home-router 的 192.168.1.0/24” */
const routesText = computed(() => {
  const list = props.overview.subnetRoutes;
  if (list.length === 0) return "家里局域网";
  return list.map((r) => `${r.device} 的 ${r.routes.join("、")}`).join("，");
});

const options = computed(() => [
  {
    on: false,
    icon: House,
    title: "在家模式",
    desc: "在家里局域网时用。访问家里的设备直接走局域网，不绕进隧道。",
  },
  {
    on: true,
    icon: Plane,
    title: "出门模式",
    desc: `在外面时用。通过 ${routesText.value} 访问家里局域网的设备。`,
  },
]);
</script>

<template>
  <UiCard title="在家 / 出门模式" :icon="Route">
    <template #actions>
      <span class="ml-auto text-xs text-muted">
        <template v-if="mode === null">读取不到当前模式</template>
        <template v-else>子网路由 accept-routes {{ mode ? "已开启" : "已关闭" }}</template>
      </span>
      <LoaderCircle v-if="busy" class="size-4 animate-spin text-muted" />
    </template>

    <div class="flex flex-col gap-3">
      <!-- 位置识别：单独一行，文字长时自动换行 -->
      <div
        v-if="locationText"
        class="flex items-start gap-2 rounded-ctl bg-subtle px-3 py-2 text-xs leading-relaxed text-muted"
        title="按默认网关自动识别，可在“设置”中修改家里网关"
      >
        <MapPin class="mt-0.5 size-3.5 shrink-0 text-accent" />
        <span class="min-w-0 break-words">{{ locationText }}</span>
      </div>
      <!-- 读不到网关时退回手动标记 -->
      <label
        v-else
        class="flex cursor-pointer items-center gap-2 rounded-ctl bg-subtle px-3 py-2 text-xs text-muted"
      >
        <MapPin class="size-3.5 shrink-0 text-accent" />
        <span class="min-w-0 flex-1">
          我现在在家（{{
            location.gateways.length === 0
              ? "读不到网关，请手动标记"
              : "还没设置家里路由器地址，可在“设置 → 在家识别”中选当前网关，之后自动识别"
          }}）
        </span>
        <UiSwitch :model-value="settings?.atHome === true" @update:model-value="emit('setAtHome', $event || null)" />
      </label>
      <!-- 在家却开着出门模式：2026-09-20 踩过的坑 -->
      <UiCallout v-if="atHome === true && mode === true" tone="error">
        你在家，却开着出门模式：家里局域网的流量会绕进隧道，可能变慢甚至断网。在家应关闭子网路由。
        <template #action>
          <UiButton variant="danger" size="sm" :disabled="busy" @click="emit('fixHome')">一键改回在家模式</UiButton>
        </template>
      </UiCallout>
      <!-- 在外面却没开出门模式：访问不到家里局域网 -->
      <UiCallout v-else-if="atHome === false && mode === false" tone="punching">
        你在外面，但没开出门模式：访问不到家里局域网的设备。
        <template #action>
          <UiButton size="sm" :disabled="busy" @click="emit('request', true)">切换到出门模式</UiButton>
        </template>
      </UiCallout>

      <div class="grid grid-cols-2 gap-3">
        <button
          v-for="o in options"
          :key="o.title"
          type="button"
          class="group relative flex items-start gap-3 overflow-hidden rounded-ctl border p-3.5 text-left transition-all duration-200 disabled:cursor-default"
          :class="
            mode === o.on
              ? 'border-transparent'
              : 'border-line bg-surface hover:-translate-y-px hover:border-[color-mix(in_srgb,var(--md-accent)_40%,var(--md-line))] hover:shadow-card'
          "
          :style="
            mode === o.on
              ? {
                  background: 'linear-gradient(var(--md-surface), var(--md-surface)) padding-box, var(--md-grad) border-box',
                  boxShadow: '0 0 0 4px var(--md-accent-soft)',
                }
              : undefined
          "
          :disabled="busy || mode === o.on || mode === null"
          :title="mode === o.on ? '当前就是这个模式' : `切换到${o.title}`"
          @click="emit('request', o.on)"
        >
          <span
            class="grid size-9 shrink-0 place-items-center rounded-ctl transition-colors"
            :class="mode === o.on ? 'text-[var(--md-grad-fg)]' : 'bg-subtle text-muted group-hover:text-accent'"
            :style="mode === o.on ? { background: 'var(--md-grad)' } : undefined"
          >
            <component :is="o.icon" class="size-5" />
          </span>
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-2 text-sm font-semibold">
              {{ o.title }}
              <span v-if="mode === o.on" class="inline-flex items-center gap-1 text-xs font-medium text-accent">
                <CircleCheck class="size-3.5" />当前
              </span>
            </span>
            <span class="mt-1 block text-xs leading-relaxed text-muted">{{ o.desc }}</span>
          </span>
        </button>
      </div>
    </div>
  </UiCard>
</template>
