<script setup lang="ts">
import { computed } from "vue";
import { House, LoaderCircle, Plane, TriangleAlert } from "@lucide/vue";
import type { Overview, Settings } from "../api";

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
const atHome = computed(() => props.settings?.atHome ?? null);

/** 子网路由说明，例如“phone-router 的 192.168.66.0/24” */
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
  <section class="flex flex-col gap-3 rounded-card border border-line bg-surface p-4 shadow-card">
    <header class="flex items-center gap-3">
      <h2 class="text-sm font-semibold">在家 / 出门模式</h2>
      <span class="text-xs text-muted">
        当前：
        <template v-if="mode === null">读取不到</template>
        <template v-else>{{ mode ? "出门模式" : "在家模式" }}（子网路由 accept-routes {{ mode ? "已开启" : "已关闭" }}）</template>
      </span>
      <LoaderCircle v-if="busy" class="size-4 animate-spin text-muted" />
      <label class="ml-auto inline-flex cursor-pointer items-center gap-2 text-xs text-muted">
        <input
          type="checkbox"
          class="size-4 accent-[var(--md-accent)]"
          :checked="atHome === true"
          @change="emit('setAtHome', ($event.target as HTMLInputElement).checked || null)"
        />
        我现在在家（用来提醒模式开错）
      </label>
    </header>

    <!-- 在家却开着出门模式：2026-09-20 踩过的坑 -->
    <div
      v-if="atHome === true && mode === true"
      class="flex items-center gap-3 rounded-ctl px-3 py-2.5 text-sm"
      style="color: var(--md-error); background: color-mix(in srgb, var(--md-error) 12%, transparent)"
    >
      <TriangleAlert class="size-5 shrink-0" />
      <span class="flex-1">你在家，却开着出门模式：家里局域网的流量会绕进隧道，可能变慢甚至断网。在家应关闭子网路由。</span>
      <button
        class="shrink-0 rounded-ctl px-3 py-1 text-white disabled:opacity-60"
        style="background: var(--md-error)"
        :disabled="busy"
        @click="emit('fixHome')"
      >
        一键改回在家模式
      </button>
    </div>

    <div class="grid grid-cols-2 gap-3">
      <button
        v-for="o in options"
        :key="o.title"
        class="flex items-start gap-3 rounded-ctl border p-3 text-left transition-colors disabled:cursor-default"
        :class="mode === o.on ? 'border-accent bg-subtle' : 'border-line hover:bg-subtle'"
        :disabled="busy || mode === o.on || mode === null"
        :title="mode === o.on ? '当前就是这个模式' : `切换到${o.title}`"
        @click="emit('request', o.on)"
      >
        <component :is="o.icon" class="mt-0.5 size-5 shrink-0" :class="mode === o.on ? 'text-accent' : 'text-muted'" />
        <span>
          <span class="flex items-center gap-2 text-sm font-medium">
            {{ o.title }}
            <span v-if="mode === o.on" class="rounded-full bg-accent px-1.5 text-[11px] text-accent-fg">当前</span>
          </span>
          <span class="mt-1 block text-xs leading-relaxed text-muted">{{ o.desc }}</span>
        </span>
      </button>
    </div>
  </section>
</template>
