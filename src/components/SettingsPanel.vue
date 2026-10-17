<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Monitor, Moon, Sun } from "@lucide/vue";
import { getAppInfo, type AppInfo, type Settings, type ThemeMode } from "../api";

const props = defineProps<{ settings: Settings | null }>();
const emit = defineEmits<{ save: [settings: Settings] }>();

const info = ref<AppInfo | null>(null);
onMounted(async () => (info.value = await getAppInfo()));

const themes: { id: ThemeMode; label: string; icon: unknown }[] = [
  { id: "system", label: "跟随系统", icon: Monitor },
  { id: "light", label: "浅色", icon: Sun },
  { id: "dark", label: "深色", icon: Moon },
];

/** 改一项，整体保存 */
function patch(change: Partial<Settings>) {
  if (props.settings) emit("save", { ...props.settings, ...change });
}
</script>

<template>
  <section v-if="settings" class="mx-auto flex max-w-3xl flex-col gap-5">
    <!-- 外观 -->
    <div class="flex flex-col gap-3 rounded-card border border-line bg-surface p-4 shadow-card">
      <h2 class="text-sm font-semibold">外观</h2>
      <div class="inline-flex w-fit gap-1 rounded-ctl bg-subtle p-1">
        <button
          v-for="t in themes"
          :key="t.id"
          class="inline-flex items-center gap-1.5 rounded-ctl px-3 py-1 text-sm"
          :class="settings.theme === t.id ? 'bg-surface font-medium shadow-card' : 'text-muted hover:text-fg'"
          @click="patch({ theme: t.id })"
        >
          <component :is="t.icon" class="size-4" />
          {{ t.label }}
        </button>
      </div>
    </div>

    <!-- 启动 -->
    <div class="flex flex-col gap-2 rounded-card border border-line bg-surface p-4 shadow-card">
      <h2 class="text-sm font-semibold">启动</h2>
      <label class="inline-flex w-fit cursor-pointer items-center gap-2 text-sm">
        <input
          type="checkbox"
          class="size-4 accent-[var(--md-accent)]"
          :checked="settings.launchOnBoot"
          @change="patch({ launchOnBoot: ($event.target as HTMLInputElement).checked })"
        />
        开机时自动启动（只在托盘显示，不弹出窗口）
      </label>
      <p class="text-xs text-muted">
        免安装版挪到别的位置后，需要重新打开一次程序才会更新开机启动的路径。
        <template v-if="info?.debug">当前是开发版，勾选只会保存设置，不会真正写入开机启动。</template>
      </p>
    </div>

    <!-- 关于 -->
    <dl
      v-if="info"
      class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 rounded-card border border-line bg-surface p-4 text-sm shadow-card"
    >
      <dt class="text-muted">版本</dt>
      <dd class="num">{{ info.version }}{{ info.debug ? "（开发版）" : "" }}</dd>
      <dt class="text-muted">运行方式</dt>
      <dd>{{ info.portable ? "便携模式（数据在程序旁的 data 文件夹）" : "数据保存在用户文件夹" }}</dd>
      <dt class="text-muted">设置文件</dt>
      <dd class="num break-all select-text">{{ info.settingsFile }}</dd>
    </dl>
  </section>
</template>
