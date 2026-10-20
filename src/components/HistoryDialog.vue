<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { ChartLine, Download, LoaderCircle } from "@lucide/vue";
import { exportHistory, getDeviceHistory, type Device, type Sample } from "../api";
import { formatLatency, latencyStats } from "../format";
import LatencyChart from "./LatencyChart.vue";
import UiButton from "./ui/UiButton.vue";
import UiDialog from "./ui/UiDialog.vue";
import UiSegmented from "./ui/UiSegmented.vue";

/** 一台设备的延迟历史详情：24 小时 / 7 天大图、统计、导出 CSV */
const props = defineProps<{
  device: Device;
  now: number;
  /** 卡片上的 24 小时历史：有新测量时随之重新读取 7 天的数据 */
  recent?: Sample[];
}>();
const emit = defineEmits<{ close: [] }>();

const DAY = 24 * 3600 * 1000;
const spans = [
  { value: DAY, label: "24 小时" },
  { value: 7 * DAY, label: "7 天" },
];
const span = ref(DAY);

const all = ref<Sample[] | null>(null);
const error = ref<string | null>(null);
/** 导出结果提示 */
const notice = ref<string | null>(null);

async function load() {
  try {
    all.value = await getDeviceHistory(props.device.id);
    error.value = null;
  } catch (e) {
    error.value = `读取失败：${String(e)}`;
  }
}
onMounted(load);
watch(() => props.recent?.length, load);

const shown = computed(() => (all.value ?? []).filter((s) => s.atMs >= props.now - span.value));
const stats = computed(() => latencyStats(shown.value));
const ms = (v: number | null) => (v == null ? "—" : formatLatency(v));
const cells = computed(() => {
  const st = stats.value;
  return [
    { label: "测量次数", value: String(st.count) },
    { label: "中位延迟", value: ms(st.median) },
    { label: "95% 不超过", value: ms(st.p95) },
    { label: "最低 / 最高", value: st.min == null ? "—" : `${ms(st.min)} / ${ms(st.max)}` },
    { label: "走中继", value: st.relayRatio == null ? "—" : `${Math.round(st.relayRatio * 100)}%` },
    { label: "没有回应", value: `${st.timeouts} 次` },
  ];
});

async function doExport() {
  try {
    notice.value = `已保存：${await exportHistory(props.device.id)}`;
  } catch (e) {
    notice.value = String(e);
  }
}
</script>

<template>
  <UiDialog
    :title="`${device.name} 的延迟历史`"
    subtitle="后台测速和手动测试的结果，只保存在本机，保留 7 天"
    :icon="ChartLine"
    width="3xl"
    close-button
    @close="emit('close')"
  >
    <div class="flex items-center gap-3">
      <UiSegmented v-model="span" :options="spans" />
    </div>

    <div v-if="!all && !error" class="flex items-center justify-center gap-2 py-16 text-muted">
      <LoaderCircle class="size-5 animate-spin" />正在读取…
    </div>
    <p v-else-if="error" class="py-8 text-center text-error">{{ error }}</p>
    <template v-else>
      <p v-if="shown.length === 0" class="py-10 text-center text-sm text-muted">这段时间里还没有测量</p>
      <LatencyChart v-else :samples="shown" :now="now" :span="span" :height="180" axis />
      <dl class="grid grid-cols-3 gap-3">
        <div v-for="c in cells" :key="c.label" class="rounded-ctl bg-subtle px-3 py-2">
          <dt class="text-[11px] text-muted">{{ c.label }}</dt>
          <dd class="num mt-0.5 text-base font-semibold">{{ c.value }}</dd>
        </div>
      </dl>
    </template>

    <template #footer>
      <span class="min-w-0 flex-1 truncate text-xs text-muted" :title="notice ?? undefined">{{ notice }}</span>
      <UiButton size="sm" variant="primary" :icon="Download" :disabled="!all?.length" @click="doExport">
        导出 7 天 CSV
      </UiButton>
    </template>
  </UiDialog>
</template>
