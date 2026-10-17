<script setup lang="ts">
import { computed, ref } from "vue";
import { CircleAlert, CircleCheck, CircleX, FileText, Info, LoaderCircle, Stethoscope } from "@lucide/vue";
import { runNetcheck, type CliError, type Level, type NetcheckReport } from "../api";
import { clock, formatLatency } from "../format";
import ErrorView from "./ErrorView.vue";
import ReportDialog from "./ReportDialog.vue";

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

/** 结论级别 → 图标和状态色 */
const levelStyle: Record<Level, { icon: unknown; color: string }> = {
  good: { icon: CircleCheck, color: "var(--md-direct)" },
  warn: { icon: CircleAlert, color: "var(--md-punching)" },
  bad: { icon: CircleX, color: "var(--md-error)" },
  info: { icon: Info, color: "var(--md-idle)" },
};

/** 官方中继默认折叠，只露出自建中继 */
const selfHosted = computed(() => report.value?.regions.filter((r) => r.selfHosted) ?? []);
const official = computed(() => report.value?.regions.filter((r) => !r.selfHosted) ?? []);
</script>

<template>
  <section class="mx-auto flex max-w-3xl flex-col gap-5">
    <div class="flex items-center gap-3">
      <div>
        <h2 class="text-base font-semibold">本机网络体检</h2>
        <p class="mt-0.5 text-xs text-muted">
          检查当前网络能不能和其他设备直连：UDP、IPv6、NAT 类型、到中继的延迟。大约需要 5 秒。
        </p>
      </div>
      <button
        class="ml-auto inline-flex shrink-0 items-center gap-1.5 rounded-ctl border border-line px-3 py-1.5 text-sm hover:bg-subtle"
        title="汇总状态、体检结果和设置，公网地址自动打码，方便发给运维"
        @click="showReport = true"
      >
        <FileText class="size-4" />
        导出诊断报告
      </button>
      <button
        class="inline-flex shrink-0 items-center gap-1.5 rounded-ctl bg-accent px-4 py-1.5 text-sm text-accent-fg hover:bg-accent-hover disabled:opacity-60"
        :disabled="busy"
        @click="run"
      >
        <LoaderCircle v-if="busy" class="size-4 animate-spin" />
        <Stethoscope v-else class="size-4" />
        {{ busy ? "正在体检…" : report ? "重新体检" : "开始体检" }}
      </button>
    </div>
    <ReportDialog v-if="showReport" @close="showReport = false" />

    <ErrorView v-if="error && !busy" :error="error" :busy="busy" @retry="run" />

    <p v-else-if="!report" class="py-16 text-center text-muted">
      {{ busy ? "正在检查网络，请稍候…" : "还没有体检过，点右上角“开始体检”" }}
    </p>

    <template v-else>
      <!-- 结论 -->
      <ul class="flex flex-col gap-2 rounded-card border border-line bg-surface p-4 shadow-card">
        <li v-for="(f, i) in report.findings" :key="i" class="flex items-start gap-2.5 leading-relaxed">
          <component
            :is="levelStyle[f.level].icon"
            class="mt-0.5 size-4 shrink-0"
            :style="{ color: levelStyle[f.level].color }"
          />
          <span>{{ f.text }}</span>
        </li>
      </ul>

      <!-- 明细 -->
      <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 rounded-card border border-line bg-surface p-4 text-sm shadow-card">
        <dt class="text-muted">IPv4 公网出口</dt>
        <dd class="num select-text">{{ report.globalV4 ?? "无" }}</dd>
        <dt class="text-muted">IPv6 公网出口</dt>
        <dd class="num break-all select-text">{{ report.globalV6 ?? "无" }}</dd>
        <dt class="text-muted">端口映射</dt>
        <dd>{{ report.portMapping.length ? report.portMapping.join(" / ") : "未开启" }}</dd>
        <dt class="text-muted">自建中继</dt>
        <dd>
          <span v-for="r in selfHosted" :key="r.id" class="num">
            {{ r.name }} {{ formatLatency(r.ms) }}{{ r.preferred ? "（首选）" : "" }}
          </span>
          <span v-if="selfHosted.length === 0">没有测到</span>
        </dd>
        <dt class="text-muted">体检时间</dt>
        <dd class="num">{{ clock(report.atMs) }}</dd>
      </dl>

      <details v-if="official.length" class="rounded-card border border-line bg-surface p-4 text-sm shadow-card">
        <summary class="cursor-pointer text-muted select-none">
          官方中继区域延迟（{{ official.length }} 个，仅供参考）
        </summary>
        <ul class="mt-3 grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-x-6 gap-y-1">
          <li v-for="r in official" :key="r.id" class="flex justify-between gap-2">
            <span class="truncate">{{ r.name }}{{ r.preferred ? "（首选）" : "" }}</span>
            <span class="num text-muted">{{ formatLatency(r.ms) }}</span>
          </li>
        </ul>
      </details>
    </template>
  </section>
</template>
