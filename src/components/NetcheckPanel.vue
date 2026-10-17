<script setup lang="ts">
import { computed, ref, type Component } from "vue";
import { CircleAlert, CircleCheck, CircleX, FileText, Info, ListChecks, Radar, Stethoscope } from "@lucide/vue";
import { runNetcheck, type CliError, type Level, type NetcheckReport } from "../api";
import { clock, formatLatency } from "../format";
import ErrorView from "./ErrorView.vue";
import ReportDialog from "./ReportDialog.vue";
import UiButton from "./ui/UiButton.vue";
import UiCard from "./ui/UiCard.vue";

const report = ref<NetcheckReport | null>(null);
const error = ref<CliError | null>(null);
const busy = ref(false);
const showReport = ref(false);

async function run() {
  if (busy.value) return;
  busy.value = true;
  try {
    report.value = await runNetcheck();
    error.value = null;
  } catch (e) {
    error.value = e as CliError;
  } finally {
    busy.value = false;
  }
}

/** 结论级别 → 图标、状态色名称、排序（数字小的排前面） */
const levelStyle: Record<Level, { icon: Component; tone: string; order: number }> = {
  bad: { icon: CircleX, tone: "error", order: 0 },
  warn: { icon: CircleAlert, tone: "punching", order: 1 },
  info: { icon: Info, tone: "idle", order: 2 },
  good: { icon: CircleCheck, tone: "direct", order: 3 },
};

/** 有问题的结论排在最前；同级保持原顺序 */
const findings = computed(() =>
  [...(report.value?.findings ?? [])].sort((a, b) => levelStyle[a.level].order - levelStyle[b.level].order),
);

/** 官方中继默认折叠，只露出自建中继 */
const selfHosted = computed(() => report.value?.regions.filter((r) => r.selfHosted) ?? []);
const official = computed(() => report.value?.regions.filter((r) => !r.selfHosted) ?? []);
</script>

<template>
  <section class="mx-auto flex max-w-3xl flex-col gap-5">
    <div class="flex items-center gap-3">
      <span class="grid size-10 shrink-0 place-items-center rounded-card text-[var(--md-grad-fg)]" style="background: var(--md-grad)">
        <Stethoscope class="size-5" />
      </span>
      <div class="min-w-0">
        <h2 class="text-base font-semibold">本机网络体检</h2>
        <p class="mt-0.5 text-xs text-muted">
          检查当前网络能不能和其他设备直连：UDP、IPv6、NAT 类型、到中继的延迟。大约需要 5 秒。
        </p>
      </div>
      <UiButton
        class="ml-auto"
        size="sm"
        :icon="FileText"
        title="汇总状态、体检结果和设置，公网地址自动打码，方便发给运维"
        @click="showReport = true"
      >
        导出诊断报告
      </UiButton>
      <UiButton variant="primary" :icon="Radar" :loading="busy" :disabled="busy" @click="run">
        {{ busy ? "正在体检…" : report ? "重新体检" : "开始体检" }}
      </UiButton>
    </div>
    <ReportDialog v-if="showReport" @close="showReport = false" />

    <ErrorView v-if="error && !busy" :error="error" :busy="busy" @retry="run" />

    <!-- 第一次体检进行中：骨架占位 -->
    <div v-else-if="!report && busy" class="flex flex-col gap-5" aria-label="正在检查网络，请稍候">
      <div class="ui-card flex flex-col gap-3 p-4">
        <div v-for="n in 4" :key="n" class="flex items-center gap-3">
          <span class="size-5 animate-pulse rounded-full bg-subtle" />
          <span class="h-3.5 animate-pulse rounded bg-subtle" :style="{ width: `${85 - n * 12}%` }" />
        </div>
      </div>
      <div class="grid grid-cols-3 gap-3">
        <div v-for="n in 3" :key="n" class="ui-card flex flex-col gap-2 p-4">
          <span class="h-3 w-16 animate-pulse rounded bg-subtle" />
          <span class="h-5 w-28 animate-pulse rounded bg-subtle" />
        </div>
      </div>
    </div>

    <div v-else-if="!report" class="ui-card flex flex-col items-center gap-3 py-14 text-center">
      <span class="grid size-12 place-items-center rounded-full text-accent" style="background: var(--md-accent-soft)">
        <Radar class="size-6" />
      </span>
      <p class="text-sm text-muted">还没有体检过，点右上角“开始体检”</p>
    </div>

    <template v-else>
      <!-- 结论：有问题的排前面，按级别着色 -->
      <UiCard title="体检结论" :icon="ListChecks">
        <ul class="flex flex-col gap-2">
          <li
            v-for="(f, i) in findings"
            :key="i"
            class="flex items-start gap-2.5 rounded-ctl px-3 py-2 text-sm leading-relaxed"
            :style="{ background: `var(--md-${levelStyle[f.level].tone}-soft)` }"
          >
            <component
              :is="levelStyle[f.level].icon"
              class="mt-0.5 size-4 shrink-0"
              :style="{ color: `var(--md-${levelStyle[f.level].tone})` }"
            />
            <span>{{ f.text }}</span>
          </li>
        </ul>
      </UiCard>

      <!-- 明细：数据小卡片 -->
      <div class="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
        <div class="ui-card flex flex-col gap-1 p-4">
          <span class="text-xs text-muted">IPv4 公网出口</span>
          <span class="num text-sm font-medium select-text">{{ report.globalV4 ?? "无" }}</span>
        </div>
        <div class="ui-card flex flex-col gap-1 p-4">
          <span class="text-xs text-muted">IPv6 公网出口</span>
          <span class="num text-sm font-medium break-all select-text">{{ report.globalV6 ?? "无" }}</span>
        </div>
        <div class="ui-card flex flex-col gap-1 p-4">
          <span class="text-xs text-muted">端口映射</span>
          <span class="text-sm font-medium">{{ report.portMapping.length ? report.portMapping.join(" / ") : "未开启" }}</span>
        </div>
        <div class="ui-card flex flex-col gap-1 p-4">
          <span class="text-xs text-muted">自建中继</span>
          <span v-for="r in selfHosted" :key="r.id" class="num text-sm font-medium">
            {{ r.name }} {{ formatLatency(r.ms) }}{{ r.preferred ? "（首选）" : "" }}
          </span>
          <span v-if="selfHosted.length === 0" class="text-sm font-medium">没有测到</span>
        </div>
        <div class="ui-card flex flex-col gap-1 p-4">
          <span class="text-xs text-muted">体检时间</span>
          <span class="num text-sm font-medium">{{ clock(report.atMs) }}</span>
        </div>
      </div>

      <details v-if="official.length" class="ui-card p-4 text-sm">
        <summary class="cursor-pointer text-muted select-none hover:text-fg">
          官方中继区域延迟（{{ official.length }} 个，仅供参考）
        </summary>
        <ul class="mt-3 grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-x-6 gap-y-1">
          <li v-for="r in official" :key="r.id" class="flex justify-between gap-2 border-b border-line/60 py-1">
            <span class="truncate">{{ r.name }}{{ r.preferred ? "（首选）" : "" }}</span>
            <span class="num text-muted">{{ formatLatency(r.ms) }}</span>
          </li>
        </ul>
      </details>
    </template>
  </section>
</template>
