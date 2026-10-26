<script setup lang="ts">
import { onMounted, ref, type Component } from "vue";
import { Bell, Check, Clock, House, Info, Monitor, Moon, Palette, Power, Server, Sun } from "@lucide/vue";
import {
  getAppInfo,
  openAppRelease,
  type AppInfo,
  type AppUpdateState,
  type LocalPrefs,
  type PrefSwitch,
  type Settings,
  type Snapshot,
  type ThemeMode,
} from "../api";
import { PALETTES } from "../palette";
import AccountsCard from "./AccountsCard.vue";
import NetPrefsCard from "./NetPrefsCard.vue";
import UiCard from "./ui/UiCard.vue";
import UiSegmented from "./ui/UiSegmented.vue";
import UiSwitch from "./ui/UiSwitch.vue";

/** gateways：当前读到的默认网关，方便直接选作家里网关；prefs：本机组网选项（存在 tailscale 里，不在设置文件） */
const props = defineProps<{
  settings: Settings | null;
  gateways: string[];
  prefs: LocalPrefs | null;
  prefBusy: boolean;
  /** MeshDock 自身的新版本检查结果 */
  appUpdate: AppUpdateState | null;
}>();
const emit = defineEmits<{
  save: [change: Partial<Settings>];
  togglePref: [pref: PrefSwitch, on: boolean, ok: string];
  /** 切换账户成功：带回新快照 */
  switched: [snapshot: Snapshot, name: string];
}>();

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
/** 与 settings.rs 中的 LATENCY_ALERT_CHOICES、RELAY_ALERT_CHOICES 一致，0 为关闭 */
const latencyAlertChoices = [0, 100, 200, 500].map((ms) => ({ value: ms, label: ms ? `${ms} ms` : "关闭" }));
const relayAlertChoices = [0, 10, 30, 60].map((m) => ({ value: m, label: m ? `${m} 分钟` : "关闭" }));

const openError = ref<string | null>(null);
async function openRelease() {
  try {
    await openAppRelease();
  } catch (e) {
    openError.value = String(e);
  }
}

/** 改一项，由 App 合并到最新设置后整体保存 */
function patch(change: Partial<Settings>) {
  emit("save", change);
}

// ---------- 家里网关 ----------
const gatewayError = ref<string | null>(null);
const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;

function saveGateway(value: string) {
  const v = value.trim();
  // 清空表示不按网关识别位置
  if (v && !IPV4.test(v)) {
    gatewayError.value = "请填写 IPv4 地址，例如 192.168.1.1";
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

      <div class="mt-3 border-t border-line pt-3">
        <p class="text-sm">配色方案</p>
        <p class="mt-0.5 text-xs text-muted">换的是品牌色和背景色调，浅色、深色都适用；连接状态的颜色（绿、橙、黄、红、灰）不变。</p>
        <div class="mt-3 grid grid-cols-5 gap-3" role="radiogroup" aria-label="配色方案">
          <button
            v-for="p in PALETTES"
            :key="p.id"
            type="button"
            role="radio"
            :aria-checked="settings.palette === p.id"
            class="group flex flex-col gap-2 rounded-ctl border p-2 text-left transition-all duration-150"
            :class="
              settings.palette === p.id
                ? 'border-accent'
                : 'border-line hover:-translate-y-px hover:border-[color-mix(in_srgb,var(--md-accent)_40%,var(--md-line))]'
            "
            :style="settings.palette === p.id ? { boxShadow: '0 0 0 3px var(--md-accent-soft)' } : undefined"
            @click="patch({ palette: p.id })"
          >
            <!-- 小样：按该方案的深色效果画一个缩小的窗口 -->
            <span class="relative block h-14 overflow-hidden rounded-[7px]" :style="{ background: p.swatch.bg }">
              <span
                class="absolute top-2 left-2 size-3.5 rounded"
                :style="{ background: `linear-gradient(135deg, ${p.swatch.from}, ${p.swatch.to})` }"
              />
              <span class="absolute top-2.5 left-7 h-1.5 w-8 rounded-full bg-white/25" />
              <span class="absolute inset-x-2 bottom-2 flex h-5 items-center gap-1 rounded px-1.5" :style="{ background: p.swatch.card }">
                <span class="size-1.5 rounded-full bg-[#4ade80]" />
                <span class="h-1 w-6 rounded-full bg-white/20" />
                <span
                  class="ml-auto h-2.5 w-5 rounded-sm"
                  :style="{ background: `linear-gradient(135deg, ${p.swatch.from}, ${p.swatch.to})` }"
                />
              </span>
              <span
                v-if="settings.palette === p.id"
                class="absolute top-1.5 right-1.5 grid size-4 place-items-center rounded-full bg-white text-[#18181b]"
              >
                <Check class="size-3" />
              </span>
            </span>
            <span class="px-0.5">
              <span class="block text-sm font-medium">{{ p.label }}</span>
              <span class="block text-[11px] text-muted">{{ p.desc }}</span>
            </span>
          </button>
        </div>
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

    <!-- 组网选项：直接改 tailscale 的设置 -->
    <NetPrefsCard :prefs="prefs" :busy="prefBusy" @toggle="(p, on, ok) => emit('togglePref', p, on, ok)" />

    <!-- 账户：多个组网之间切换 -->
    <AccountsCard @switched="(s, name) => emit('switched', s, name)" />

    <!-- 位置识别 -->
    <UiCard title="在家识别" :icon="House">
      <p class="text-sm">家里路由器地址</p>
      <p class="mt-0.5 text-xs text-muted">
        本机的默认网关等于这个地址时，视为在家。在家时点下面的“当前默认网关”即可填入；留空则不自动识别位置。模式和位置不一致时，设备页会提示；开启下面的“自动切换”后会自动改过来。组网里没有子网路由器时，这一项用不到。
      </p>
      <!-- 输入框单独一行 -->
      <input
        class="ui-input num mt-2.5 block w-56 text-sm select-text"
        :value="settings.homeGateway"
        placeholder="例如 192.168.1.1"
        @change="saveGateway(($event.target as HTMLInputElement).value)"
        @keydown.enter="($event.target as HTMLInputElement).blur()"
      />
      <p v-if="gatewayError" class="mt-2 text-xs text-error">{{ gatewayError }}</p>
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
      <label class="mt-3 flex cursor-pointer items-center gap-4 border-t border-line pt-3">
        <div class="min-w-0 flex-1">
          <p class="text-sm">自动切换在家 / 出门模式</p>
          <p class="mt-0.5 text-xs text-muted">
            按默认网关识别到回家或出门（连续两次刷新结果一致）后，模式不对就自动切换一次，并发通知告知。之后手动改的模式会保留，直到位置再次变化；读不到网关、只能手动标记时不自动切换。
          </p>
        </div>
        <UiSwitch :model-value="settings.autoMode" @update:model-value="patch({ autoMode: $event })" />
      </label>
    </UiCard>

    <!-- 自建中继的显示名 -->
    <UiCard title="自建中继" :icon="Server">
      <p class="text-sm">显示名称</p>
      <p class="mt-0.5 text-xs text-muted">
        设备走自建中继时显示为“中继（经 这个名称）”，例如填中继服务器所在的城市。留空时用控制端下发的名称，没有就显示“自建”。
      </p>
      <input
        class="ui-input mt-2.5 block w-56 text-sm select-text"
        :value="settings.relayName"
        maxlength="16"
        placeholder="例如 广州"
        @change="patch({ relayName: ($event.target as HTMLInputElement).value.trim() })"
        @keydown.enter="($event.target as HTMLInputElement).blur()"
      />
    </UiCard>

    <!-- 通知 -->
    <UiCard title="通知" :icon="Bell">
      <div class="flex flex-col divide-y divide-line">
        <label class="flex cursor-pointer items-center gap-4 pb-3">
          <div class="min-w-0 flex-1">
            <p class="text-sm">发送 Windows 通知</p>
            <p class="mt-0.5 text-xs text-muted">
              设备在直连和中继之间变化（新路径持续 30 秒以上，打洞时的短暂中继不算），或在家 / 出门模式与所在位置不一致时提醒。下面两项提醒也受这个开关控制。
            </p>
          </div>
          <UiSwitch :model-value="settings.notify" @update:model-value="patch({ notify: $event })" />
        </label>
        <div class="flex items-center gap-4 py-3" :class="{ 'opacity-50': !settings.notify }">
          <div class="min-w-0 flex-1">
            <p class="text-sm">延迟持续偏高时提醒</p>
            <p class="mt-0.5 text-xs text-muted">某台设备连续 3 次后台测速都超过这个值时提醒一次，回落后才会再次提醒。</p>
          </div>
          <UiSegmented
            mono
            :options="latencyAlertChoices"
            :model-value="settings.latencyAlertMs"
            @update:model-value="patch({ latencyAlertMs: $event })"
          />
        </div>
        <div class="flex items-center gap-4 pt-3" :class="{ 'opacity-50': !settings.notify }">
          <div class="min-w-0 flex-1">
            <p class="text-sm">长时间走中继时提醒</p>
            <p class="mt-0.5 text-xs text-muted">某台设备连续走中继超过这么久时提醒一次（中继限速约 500 KB/s），恢复直连后重新计时。</p>
          </div>
          <UiSegmented
            mono
            :options="relayAlertChoices"
            :model-value="settings.relayAlertMinutes"
            @update:model-value="patch({ relayAlertMinutes: $event })"
          />
        </div>
      </div>
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
        <dt>新版本</dt>
        <dd>
          <template v-if="appUpdate?.state === 'available'">
            <span class="font-medium text-accent">有新版 {{ appUpdate.version }}</span>
            <button type="button" class="ml-2 text-accent hover:underline" @click="openRelease">打开下载页</button>
          </template>
          <template v-else-if="appUpdate?.state === 'latest'">已是最新版（每 6 小时检查一次）</template>
          <span v-else-if="appUpdate?.state === 'unavailable'" :title="appUpdate.reason">
            无法检查：需要安装并登录 GitHub 命令行 gh（仓库是私有的）
          </span>
          <template v-else>还没检查</template>
        </dd>
        <template v-if="openError"><dt /><dd class="text-error">{{ openError }}</dd></template>
        <dt>运行方式</dt>
        <dd>{{ info.portable ? "便携模式（数据在程序旁的 data 文件夹）" : "数据保存在用户文件夹" }}</dd>
        <dt>设置文件</dt>
        <dd class="num break-all select-text">{{ info.settingsFile }}</dd>
      </dl>
    </div>
  </section>
</template>
