// 底部操作提示：成功或失败由调用方明确指定，不再靠文字内容判断
import { onUnmounted, ref } from "vue";

export type ToastKind = "ok" | "error";

export interface Toast {
  text: string;
  kind: ToastKind;
}

/** 提示显示多久（毫秒） */
const DURATION_MS = 2500;

export function useToast() {
  const toast = ref<Toast | null>(null);
  let timer: number | undefined;

  function show(text: string, kind: ToastKind = "ok") {
    toast.value = { text, kind };
    clearTimeout(timer);
    timer = window.setTimeout(() => (toast.value = null), DURATION_MS);
  }

  onUnmounted(() => clearTimeout(timer));

  return {
    toast,
    showOk: (text: string) => show(text, "ok"),
    showError: (text: string) => show(text, "error"),
  };
}
