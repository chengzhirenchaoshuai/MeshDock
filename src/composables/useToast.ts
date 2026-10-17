// 底部操作提示：成功或失败由调用方明确指定，不再靠文字内容判断
import { getCurrentInstance, onUnmounted, ref } from "vue";

export type ToastKind = "ok" | "error";

export interface Toast {
  text: string;
  kind: ToastKind;
}

/** 提示显示多久（毫秒） */
const DURATION_MS = 2500;

export function useToast() {
  const toast = ref<Toast | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function show(text: string, kind: ToastKind = "ok") {
    toast.value = { text, kind };
    clearTimeout(timer);
    timer = setTimeout(() => (toast.value = null), DURATION_MS);
  }

  // 在组件中使用时，卸载后清掉计时器
  if (getCurrentInstance()) onUnmounted(() => clearTimeout(timer));

  return {
    toast,
    showOk: (text: string) => show(text, "ok"),
    showError: (text: string) => show(text, "error"),
  };
}
