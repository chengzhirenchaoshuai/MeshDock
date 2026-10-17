import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useToast } from "./useToast";

describe("useToast", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("成功和失败分别标记，2.5 秒后消失", () => {
    const { toast, showOk, showError } = useToast();
    showOk("已复制");
    expect(toast.value).toEqual({ text: "已复制", kind: "ok" });
    showError("复制失败");
    expect(toast.value).toEqual({ text: "复制失败", kind: "error" });
    vi.advanceTimersByTime(2499);
    expect(toast.value).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(toast.value).toBeNull();
  });

  it("新提示重新计时，不会被上一条的计时器提前清掉", () => {
    const { toast, showOk } = useToast();
    showOk("第一条");
    vi.advanceTimersByTime(2000);
    showOk("第二条");
    vi.advanceTimersByTime(2000);
    expect(toast.value?.text).toBe("第二条");
  });

  it("文字里含“失败”但用 showOk 时仍是成功", () => {
    const { toast, showOk } = useToast();
    showOk("失败重试已完成");
    expect(toast.value?.kind).toBe("ok");
  });
});
