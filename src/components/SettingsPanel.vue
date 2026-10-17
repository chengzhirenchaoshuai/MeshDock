<script setup lang="ts">
import { onMounted, ref, type Component } from "vue";
import { Bell, Clock, House, Info, Monitor, Moon, Palette, Power, Sun } from "@lucide/vue";
import { getAppInfo, type AppInfo, type Settings, type ThemeMode } from "../api";
import UiCard from "./ui/UiCard.vue";
import UiSegmented from "./ui/UiSegmented.vue";
import UiSwitch from "./ui/UiSwitch.vue";

/** gateways：当前读到的默认网关，方便直接选作家里网关 */
const props = defineProps<{ settings: Settings | null; gateways: string[] }>();
const emit = defineEmits<{ save: [settings: Settings] }>();

const info = ref<AppInfo | null>(null);
onMounted(async () => (info.value = await getAppInfo()));

const themes: { value: ThemeMode; label: string; icon: Component }[] = [
  { value: "system", label: "跟随系统", icon: Monitor },
  { value: "light", label: "浅色", icon: Sun },
  { value: "dark", label: "深色", icon: Moon },
];

/** 可选值与后端 settings.rs 中的 REFRESH_CHOICES、SAMPLE_CHOICES 一致 */
const refreshChoices = [5, 10, 30, 60].map((s) => ({ value: s, label: `${s} 秒` }));
const sampleChoices = [1, 2, 5, 10].map((m) => ({ value: m, label: `${m} 分钟` }));

/** 改一项，整体保存 */
function patch(change: Partial<Settings>) {
  if (props.settings) emit("save", { ...props.settings, ...change });
}

// ---------- 家里网关 ----------
const gatewayError = ref<string | null>(null);
const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;

function saveGateway(value: string) {
  const v = value.trim();
  if (!IPV4.test(v)) {
    gatewayError.value = "请填写 IPv4 地址，例如 192.168.66.66";
    return;
  }
  gatewayError.value = null;
  if (v !== props.settings?.homeGateway) patch({ homeGateway: v });
}
</script>

<template>
  <section v-if="settings" class="mx-auto flex max-w-3xl flex-col gap-5">
    <!-- 外观 -->
    <UiCard title="外观" :icon="Palette">
      <div class="flex items-center gap-4">
        <div class="min-w-0 flex-1">
          <p class="text-sm">主题</p>
          <p class="mt-0.5 text-xs text-muted">深色是主设计方向；跟随系统时随 Windows 的设置切换。</p>
        </div>
        <UiSegmented :options="themes" :model-value="settings.theme" @update:model-value="patch({ theme: $event })" />
      </div>
    </UiCard>

    <!-- 检测周期 -->
    <UiCard title="检测周期" :icon="Clock">
      <div class="flex flex-col divide-y divide-line">
        <div class="flex items-center gap-4 pb-3">
          <div class="min-w-0 flex-1">
            <p class="text-sm">状态刷新</p>
            <p class="mt-0.5 text-xs text-muted">
              窗口打开时多久读取一次组网状态。窗口关到托盘后至少 30 秒一次，只更新托盘圆点。
            </p>
          </div>
          <UiSegmented
            mono
            :options="refreshChoices"
            :model-value="settings.refreshSecs"
            @update:model-value="patch({ refreshSecs: $event })"
          />
        </div>
        <div class="flex items-center gap-4 pt-3">
          <div class="min-w-0 flex-1">
            <p class="text-sm">后台测速</p>
            <p class="mt-0.5 text-xs text-muted">多久自动测一次在线设备的延迟，结果画在设备卡片的 24 小时小图上。</p>
          </div>
          <UiSegmented
            mono
            :options="sampleChoices"
            :model-value="settings.sampleMinutes"
            @update:model-value="patch({ sampleMinutes: $event })"
          />
        </div>
      </div>
    </UiCard>

    <!-- 位置识别 -->
    <UiCard title="在家识别" :icon="House">
      <div class="flex items-center gap-4">
        <div class="min-w-0 flex-1">
          <p class="text-sm">家里路由器地址</p>
          <p class="mt-0.5 text-xs text-muted">
            本机的默认网关等于这个地址时，视为在家。模式和位置不一致时，设备页会提示，但不会自动切换。
          </p>
        </div>
        <input
          class="ui-input num w-44 text-sm select-text"
          :value="settings.homeGateway"
          @change="saveGateway(($event.target as HTMLInputElement).value)"
          @keydown.enter="($event.target as HTMLInputElement).blur()"
        />
      </div>
      <p v-if="gatewayError" class="mt-2 text-right text-xs text-error">{{ gatewayError }}</p>
      <p v-if="gateways.length" class="mt-3 flex flex-wrap items-center gap-2 border-t border-line pt-3 text-xs text-muted">
        当前默认网关：
        <button
          v-for="g in gateways"
          :key="g"
          type="button"
          class="num rounded-full border px-2.5 py-0.5 transition-colors hover:border-accent hover:text-accent"
          :class="g === settings.homeGateway ? 'border-accent text-accent' : 'border-line'"
          :style="g === settings.homeGateway ? { background: 'var(--md-accent-soft)' } : undefined"
          :title="`把 ${g} 设为家里路由器地址`"
          @click="saveGateway(g)"
        >
          {{ g }}{{ g === settings.homeGateway ? "（家里）" : "" }}
        </button>
      </p>
      <p v-else class="mt-3 border-t border-line pt-3 text-xs text-muted">当前读不到默认网关，设备页会改为手动标记。</p>
    </UiCard>

    <!-- 通知 -->
    <UiCard title="通知" :icon="Bell">
      <label class="flex cursor-pointer items-center gap-4">
        <div class="min-w-0 flex-1">
          <p class="text-sm">发送 Windows 通知</p>
          <p class="mt-0.5 text-xs text-muted">
            设备在直连和中继之间变化（新路径持续 30 秒以上，打洞时的短暂中继不算），或在家 / 出门模式与所在位置不一致时提醒。
          </p>
        </div>
        <UiSwitch :model-value="settings.notify" @update:model-value="patch({ notify: $event })" />
      </label>
    </UiCard>

    <!-- 启动与关闭 -->
    <UiCard title="启动与关闭" :icon="Power">
      <div class="flex flex-col divide-y divide-line">
        <label class="flex cursor-pointer items-center gap-4 pb-3">
          <div class="min-w-0 flex-1">
            <p class="text-sm">开机时自动启动（只在托盘显示，不弹出窗口）</p>
            <p class="mt-0.5 text-xs text-muted">
              免安装版挪到别的位置后，需要重新打开一次程序才会更新开机启动的路径。
              <template v-if="info?.debug">当前是开发版，打开只会保存设置，不会真正写入开机启动。</template>
            </p>
          </div>
          <UiSwitch :model-value="settings.launchOnBoot" @update:model-value="patch({ launchOnBoot: $event })" />
        </label>
        <label class="flex cursor-pointer items-center gap-4 pt-3">
          <div class="min-w-0 flex-1">
            <p class="text-sm">关闭窗口时最小化到系统托盘</p>
            <p class="mt-0.5 text-xs text-muted">
              打开时，点窗口右上角的关闭按钮只是隐藏到托盘，程序继续在后台检测；要退出请在托盘图标上右键选“退出”。关闭时，关闭窗口即退出程序。
            </p>
          </div>
          <UiSwitch :model-value="settings.closeToTray" @update:model-value="patch({ closeToTray: $event })" />
        </label>
      </div>
    </UiCard>

    <!-- 关于 -->
    <div v-if="info" class="flex items-start gap-3 rounded-card border border-dashed border-line px-4 py-3 text-xs text-muted">
      <Info class="mt-0.5 size-4 shrink-0 text-faint" />
      <dl class="grid min-w-0 flex-1 grid-cols-[auto_1fr] gap-x-6 gap-y-1.5">
        <dt>版本</dt>
        <dd class="num text-fg">{{ info.version }}{{ info.debug ? "（开发版）" : "" }}</dd>
        <dt>运行方式</dt>
        <dd>{{ info.portable ? "便携模式（数据在程序旁的 data 文件夹）" : "数据保存在用户文件夹" }}</dd>
        <dt>设置文件</dt>
        <dd class="num break-all select-text">{{ info.settingsFile }}</dd>
      </dl>
    </div>
  </section>
</template>
