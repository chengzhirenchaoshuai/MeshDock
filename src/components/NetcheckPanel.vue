<script setup lang="ts">
import { computed, onMounted, ref, type Component } from "vue";
import { CircleAlert, CircleCheck, CircleX, FileText, Globe, Info, ListChecks, Radar, Server, Stethoscope } from "@lucide/vue";
import { runNetcheck, type CliError, type Level, type NetcheckReport } from "../api";
import { clock, formatLatency } from "../format";
import ErrorView from "./ErrorView.vue";
import ReportDialog from "./ReportDialog.vue";
import UiButton from "./ui/UiButton.vue";
import UiCard from "./ui/UiCard.vue";
import { useSettingsJump } from "../composables/settingsJump";

const openSettings = useSettingsJump();

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

// 程序启动时自动体检一次
onMounted(run);

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

/** 顶部总评：只数真正需要处理的问题，说明类结论不算 */
const verdict = computed((): { level: Level; title: string; desc: string } => {
  const count = (level: Level) => findings.value.filter((f) => f.level === level).length;
  const bad = count("bad");
  if (bad) return { level: "bad", title: `发现 ${bad} 个问题`, desc: "可能影响设备之间直连，详情见下方结论" };
  const warn = count("warn");
  if (warn) return { level: "warn", title: `有 ${warn} 项需要留意`, desc: "不一定影响使用，详情见下方结论" };
  return { level: "good", title: "网络状况良好", desc: "没有发现影响直连的问题" };
});
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
          检查当前网络能不能和其他设备直连，大约需要 5 秒。
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
      <div class="ui-card flex items-center gap-3 p-4">
        <span class="size-10 animate-pulse rounded-full bg-subtle" />
        <span class="h-4 w-40 animate-pulse rounded bg-subtle" />
      </div>
      <div class="ui-card flex flex-col gap-3 p-4">
        <div v-for="n in 4" :key="n" class="flex items-center gap-3">
          <span class="size-5 animate-pulse rounded-full bg-subtle" />
          <span class="h-3.5 animate-pulse rounded bg-subtle" :style="{ width: `${85 - n * 12}%` }" />
        </div>
      </div>
      <div class="flex flex-col gap-5">
        <div v-for="n in 2" :key="n" class="ui-card flex flex-col gap-3 p-4">
          <span class="h-3 w-16 animate-pulse rounded bg-subtle" />
          <span class="h-4 w-full animate-pulse rounded bg-subtle" />
          <span class="h-4 w-full animate-pulse rounded bg-subtle" />
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
      <!-- 总评：一眼看到这次体检的结果 -->
      <div class="ui-card flex items-center gap-3.5 p-4" :style="{ background: `var(--md-${levelStyle[verdict.level].tone}-soft)` }">
        <component
          :is="levelStyle[verdict.level].icon"
          class="size-8 shrink-0"
          :style="{ color: `var(--md-${levelStyle[verdict.level].tone})` }"
        />
        <div class="min-w-0">
          <p class="text-base font-semibold">{{ verdict.title }}</p>
          <p class="mt-0.5 text-xs text-muted">{{ verdict.desc }}</p>
        </div>
        <span class="num ml-auto shrink-0 text-xs text-muted">体检时间 {{ clock(report.atMs) }}</span>
      </div>

      <!-- 结论：有问题的排前面并加底色，正常和说明类不加，免得满屏色块 -->
      <UiCard title="体检结论" :icon="ListChecks">
        <ul class="flex flex-col gap-1">
          <li
            v-for="(f, i) in findings"
            :key="i"
            class="flex items-start gap-2.5 rounded-ctl px-3 py-2 text-sm leading-relaxed"
            :style="f.level === 'bad' || f.level === 'warn' ? { background: `var(--md-${levelStyle[f.level].tone}-soft)` } : undefined"
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

      <!-- 明细：出口和中继各占一行 -->
      <div class="flex flex-col gap-5">
        <UiCard title="公网出口" :icon="Globe">
          <dl class="flex flex-col divide-y divide-line text-sm">
            <div class="flex items-baseline gap-4 py-2 first:pt-0">
              <dt class="w-16 shrink-0 text-xs text-muted">IPv4</dt>
              <dd class="num font-medium select-text">{{ report.globalV4 ?? "无" }}</dd>
            </div>
            <div class="flex items-baseline gap-4 py-2">
              <dt class="w-16 shrink-0 text-xs text-muted">IPv6</dt>
              <dd class="num min-w-0 font-medium break-all select-text">{{ report.globalV6 ?? "无" }}</dd>
            </div>
            <div class="flex items-baseline gap-4 py-2 last:pb-0">
              <dt class="w-16 shrink-0 text-xs text-muted">端口映射</dt>
              <dd class="font-medium">{{ report.portMapping.length ? report.portMapping.join(" / ") : "未开启" }}</dd>
            </div>
          </dl>
        </UiCard>

        <UiCard title="中继" :icon="Server">
          <template v-if="report.selfHostedExpected" #actions>
            <button type="button" class="ml-auto text-xs text-accent hover:underline" @click="openSettings('relay')">改显示名</button>
          </template>
          <p class="text-xs text-muted">自建中继</p>
          <ul v-if="selfHosted.length" class="mt-1 flex flex-col gap-1 text-sm">
            <li v-for="r in selfHosted" :key="r.id" class="flex items-baseline justify-between gap-2">
              <span class="min-w-0 truncate font-medium">{{ r.name }}{{ r.preferred ? "（首选）" : "" }}</span>
              <span class="num shrink-0">{{ formatLatency(r.ms) }}</span>
            </li>
          </ul>
          <p v-else-if="report.selfHostedExpected" class="mt-1 text-sm font-medium text-error">连不上</p>
          <template v-else>
            <p class="mt-1 text-sm font-medium">未使用</p>
            <p class="mt-0.5 text-xs leading-relaxed text-muted">这个组网没有自建中继，直连不上时走官方中继。</p>
          </template>

          <details v-if="official.length" class="mt-3 border-t border-line pt-3 text-sm">
            <summary class="cursor-pointer text-xs text-muted select-none hover:text-fg">
              官方中继延迟（{{ official.length }} 个，仅供参考）
            </summary>
            <ul class="mt-2 flex max-h-48 flex-col overflow-y-auto pr-1">
              <li v-for="r in official" :key="r.id" class="flex justify-between gap-2 py-0.5">
                <span class="truncate">{{ r.name }}{{ r.preferred ? "（首选）" : "" }}</span>
                <span class="num shrink-0 text-muted">{{ formatLatency(r.ms) }}</span>
              </li>
            </ul>
          </details>
        </UiCard>
      </div>
    </template>
  </section>
</template>
