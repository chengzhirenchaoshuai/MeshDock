<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Copy, LoaderCircle, Plus, RefreshCw, Share2, Trash2 } from "@lucide/vue";
import { addServeRule, getServeRules, removeServeRule, type CliError, type ServeKind, type ServeRule } from "../api";
import UiButton from "./ui/UiButton.vue";
import UiCard from "./ui/UiCard.vue";
import UiSegmented from "./ui/UiSegmented.vue";

/** selfIp：本机组网 IP，用来拼别人访问的地址；disabled：组网断开时不能改 */
const props = defineProps<{ selfIp: string | null; disabled: boolean }>();
const emit = defineEmits<{ ok: [text: string]; error: [text: string]; copy: [text: string] }>();

const rules = ref<ServeRule[] | null>(null);
const loadError = ref<string | null>(null);
const busy = ref(false);

async function load() {
  busy.value = true;
  try {
    rules.value = await getServeRules();
    loadError.value = null;
  } catch (e) {
    loadError.value = (e as CliError).message;
  } finally {
    busy.value = false;
  }
}
onMounted(load);

// ---------- 新增 ----------
const adding = ref(false);
const kind = ref<ServeKind>("http");
const localPort = ref("");
/** 组网端口，留空时与本机端口相同 */
const port = ref("");
const formError = ref<string | null>(null);
const kinds: { value: ServeKind; label: string }[] = [
  { value: "http", label: "网页（HTTP）" },
  { value: "tcp", label: "其他（TCP）" },
];

/** "8080" → 8080；不是 1～65535 的整数时返回 null */
function parsePort(text: string): number | null {
  const n = Number(text.trim());
  return Number.isInteger(n) && n >= 1 && n <= 65535 ? n : null;
}

async function add() {
  const local = parsePort(localPort.value);
  const outer = port.value.trim() === "" ? local : parsePort(port.value);
  if (local === null || outer === null) {
    formError.value = "端口要填 1～65535 之间的整数";
    return;
  }
  if (rules.value?.some((r) => r.port === outer)) {
    formError.value = `组网端口 ${outer} 已经在用，换一个或先删掉原来的`;
    return;
  }
  formError.value = null;
  busy.value = true;
  try {
    rules.value = await addServeRule(kind.value, outer, local);
    emit("ok", `已共享本机端口 ${local}`);
    adding.value = false;
    localPort.value = "";
    port.value = "";
  } catch (e) {
    formError.value = (e as CliError).message;
  } finally {
    busy.value = false;
  }
}

async function remove(r: ServeRule) {
  busy.value = true;
  try {
    rules.value = await removeServeRule(r.kind, r.port);
    emit("ok", `已停止共享组网端口 ${r.port}`);
  } catch (e) {
    emit("error", `停止共享失败：${(e as CliError).message}`);
  } finally {
    busy.value = false;
  }
}

/** 别人访问的地址 */
function url(r: ServeRule): string {
  const host = props.selfIp ?? "本机组网IP";
  return r.kind === "tcp" ? `${host}:${r.port}` : `${r.kind}://${host}:${r.port}`;
}
</script>

<template>
  <UiCard title="共享本机服务" :icon="Share2">
    <template #actions>
      <span class="ml-auto" />
      <LoaderCircle v-if="busy" class="size-4 animate-spin text-muted" />
      <button
        v-else
        type="button"
        class="text-muted transition-colors hover:text-accent"
        title="重新读取"
        @click="load"
      >
        <RefreshCw class="size-4" />
      </button>
    </template>

    <p class="text-xs leading-relaxed text-muted">
      把本机上只对自己开放的服务（如 127.0.0.1:8080 的网页）共享给组网里的其他设备，不用改防火墙。只在组网内可见，不会暴露到公网。
    </p>

    <p v-if="loadError" class="mt-3 text-xs text-error">读取失败：{{ loadError }}</p>
    <ul v-else-if="rules?.length" class="mt-3 flex flex-col gap-2">
      <li v-for="r in rules" :key="r.port" class="flex items-center gap-2 rounded-ctl bg-subtle px-3 py-2 text-xs">
        <span class="rounded bg-surface px-1.5 py-0.5 font-medium uppercase">{{ r.kind }}</span>
        <span class="min-w-0 flex-1">
          <button
            type="button"
            class="num group inline-flex max-w-full items-center gap-1 truncate text-fg hover:text-accent"
            title="点击复制访问地址"
            @click="emit('copy', url(r))"
          >
            {{ url(r) }}<Copy class="size-3 shrink-0 opacity-0 group-hover:opacity-100" />
          </button>
          <span class="num block truncate text-muted" :title="r.target">→ {{ r.target }}</span>
        </span>
        <button
          type="button"
          class="text-muted transition-colors hover:text-error disabled:opacity-50"
          :disabled="busy || disabled"
          :title="`停止共享组网端口 ${r.port}`"
          @click="remove(r)"
        >
          <Trash2 class="size-4" />
        </button>
      </li>
    </ul>
    <p v-else-if="rules" class="mt-3 text-xs text-faint">现在没有共享任何服务</p>

    <!-- 新增 -->
    <div v-if="adding" class="mt-3 flex flex-col gap-2.5 border-t border-line pt-3">
      <UiSegmented v-model="kind" :options="kinds" />
      <div class="flex flex-wrap items-center gap-2 text-xs text-muted">
        本机端口
        <input v-model="localPort" class="ui-input num w-20 text-sm select-text" placeholder="8080" @keydown.enter="add" />
        组网端口
        <input v-model="port" class="ui-input num w-20 text-sm select-text" placeholder="同左" @keydown.enter="add" />
      </div>
      <p v-if="formError" class="text-xs text-error">{{ formError }}</p>
      <div class="flex gap-2">
        <UiButton size="sm" variant="primary" :loading="busy" :disabled="busy || disabled" @click="add">共享</UiButton>
        <UiButton size="sm" :disabled="busy" @click="(adding = false), (formError = null)">取消</UiButton>
      </div>
    </div>
    <UiButton
      v-else
      class="mt-3"
      size="sm"
      :icon="Plus"
      :disabled="disabled || !!loadError"
      :title="disabled ? '组网断开时不能修改' : undefined"
      @click="adding = true"
    >
      共享一个端口
    </UiButton>
  </UiCard>
</template>
