<script setup lang="ts">
import { onMounted, onUnmounted, ref, type Component } from "vue";
import { X } from "@lucide/vue";

/**
 * 弹窗外壳：遮罩、出现动画、Esc 关闭、Tab 焦点不跑出弹窗
 * dismissible 为 false 时（例如正在执行）点遮罩和按 Esc 都不关闭
 */
const props = withDefaults(
  defineProps<{
    title: string;
    icon?: Component;
    /** 标题图标的状态色名称 */
    tone?: string;
    subtitle?: string;
    width?: "md" | "lg" | "3xl";
    dismissible?: boolean;
    /** 右上角显示关闭按钮 */
    closeButton?: boolean;
  }>(),
  { icon: undefined, tone: "accent", subtitle: undefined, width: "md", dismissible: true, closeButton: false },
);
const emit = defineEmits<{ close: [] }>();

const panel = ref<HTMLElement | null>(null);
const widthClass = { md: "max-w-md", lg: "max-w-lg", "3xl": "max-w-3xl" };

function focusables(): HTMLElement[] {
  if (!panel.value) return [];
  return [
    ...panel.value.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), summary, [tabindex]:not([tabindex="-1"])',
    ),
  ];
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && props.dismissible) {
    emit("close");
  } else if (e.key === "Tab") {
    const list = focusables();
    if (!list.length) return;
    const first = list[0];
    const last = list[list.length - 1];
    const inside = panel.value?.contains(document.activeElement) && document.activeElement !== panel.value;
    if (!inside) {
      (e.shiftKey ? last : first).focus();
      e.preventDefault();
    } else if (e.shiftKey && document.activeElement === first) {
      last.focus();
      e.preventDefault();
    } else if (!e.shiftKey && document.activeElement === last) {
      first.focus();
      e.preventDefault();
    }
  }
}

let previous: Element | null = null;
onMounted(() => {
  previous = document.activeElement;
  // 焦点先放到弹窗本身，按 Tab 再进入按钮，避免误按回车直接执行
  panel.value?.focus();
  window.addEventListener("keydown", onKey);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  (previous as HTMLElement | null)?.focus?.();
});
</script>

<template>
  <div
    class="fixed inset-0 z-20 grid place-items-center p-6 backdrop-blur-[2px]"
    style="background: var(--md-scrim); animation: md-fade-in 160ms var(--md-ease)"
    @click.self="dismissible && emit('close')"
  >
    <div
      ref="panel"
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
      class="ui-card flex max-h-full w-full flex-col gap-4 p-5 outline-none"
      :class="widthClass[width]"
      style="box-shadow: var(--md-shadow-dialog); animation: md-pop-in 220ms var(--md-ease)"
    >
      <header class="flex items-start gap-3">
        <span
          v-if="icon"
          class="grid size-9 shrink-0 place-items-center rounded-full"
          :style="{ color: `var(--md-${tone})`, background: `var(--md-${tone}-soft)` }"
        >
          <component :is="icon" class="size-5" />
        </span>
        <div class="min-w-0 flex-1 self-center">
          <h2 class="text-base font-semibold">{{ title }}</h2>
          <p v-if="subtitle" class="mt-0.5 text-xs text-muted">{{ subtitle }}</p>
        </div>
        <button
          v-if="closeButton"
          type="button"
          class="rounded-ctl p-1 text-muted transition-colors hover:bg-subtle hover:text-fg"
          title="关闭"
          @click="emit('close')"
        >
          <X class="size-4" />
        </button>
      </header>
      <slot />
      <footer v-if="$slots.footer" class="flex items-center justify-end gap-2">
        <slot name="footer" />
      </footer>
    </div>
  </div>
</template>
