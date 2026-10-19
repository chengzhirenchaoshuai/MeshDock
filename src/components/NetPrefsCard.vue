<script setup lang="ts">
import { computed } from "vue";
import { LoaderCircle, SlidersHorizontal } from "@lucide/vue";
import type { LocalPrefs, PrefSwitch } from "../api";
import UiCard from "./ui/UiCard.vue";
import UiSwitch from "./ui/UiSwitch.vue";

/** prefs 为 null 表示读不到（服务没运行等），开关全部禁用 */
const props = defineProps<{ prefs: LocalPrefs | null; busy: boolean }>();
/** on 是传给 tailscale 的值；ok 是成功后的提示语（按界面含义写） */
const emit = defineEmits<{ toggle: [pref: PrefSwitch, on: boolean, ok: string] }>();

/**
 * 界面上的开关。invert：界面含义与 tailscale 的参数相反
 * （“允许其他设备连接本机”打开 = shields-up 关闭，与官方客户端的 Allow incoming connections 一致）
 */
const items = computed(() => {
  const p = props.prefs;
  return [
    {
      pref: "shields-up" as const,
      invert: true,
      value: p?.shieldsUp,
      title: "允许组网里的其他设备连接本机",
      desc: "关闭后，别的设备连不上本机（SSH、RustDesk、共享文件夹、ping 都不行），本机访问别人不受影响。经组网远程控制这台电脑时不要关。",
    },
    {
      pref: "accept-dns" as const,
      invert: false,
      value: p?.acceptDns,
      title: "使用组网下发的 DNS",
      desc: "开启后可以直接用设备名（如 ming-pc）访问，并使用 Headscale 配置的 DNS。本机有代理软件接管 DNS 时，开启前先确认二者不冲突。",
    },
    {
      pref: "unattended" as const,
      invert: false,
      value: p?.unattended,
      title: "没有用户登录时也保持连接",
      desc: "注销、切换用户或刚开机还没登录时，组网保持连接，可以远程连进来。即官方客户端的 Run unattended。",
    },
  ];
});
</script>

<template>
  <UiCard title="组网选项" :icon="SlidersHorizontal">
    <template #actions>
      <span class="ml-auto text-xs text-muted">{{ prefs ? "修改后立即生效" : "读取不到当前设置" }}</span>
      <LoaderCircle v-if="busy" class="size-4 animate-spin text-muted" />
    </template>
    <div class="flex flex-col divide-y divide-line">
      <label
        v-for="(it, i) in items"
        :key="it.pref"
        class="flex items-center gap-4"
        :class="[i === 0 ? 'pb-3' : i === items.length - 1 ? 'pt-3' : 'py-3', prefs ? 'cursor-pointer' : '']"
      >
        <div class="min-w-0 flex-1">
          <p class="text-sm">{{ it.title }}</p>
          <p class="mt-0.5 text-xs text-muted">{{ it.desc }}</p>
        </div>
        <UiSwitch
          :model-value="it.value == null ? false : it.invert ? !it.value : it.value"
          :disabled="busy || it.value == null"
          @update:model-value="emit('toggle', it.pref, it.invert ? !$event : $event, `已${$event ? '开启' : '关闭'}“${it.title}”`)"
        />
      </label>
    </div>
  </UiCard>
</template>
