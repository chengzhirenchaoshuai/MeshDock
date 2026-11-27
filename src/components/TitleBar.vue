<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * 自绘标题栏：系统标题栏已去掉（tauri.conf.json 中 decorations 为 false），
 * 这里只画右上角的最小化、最大化 / 还原、关闭三个按钮，背景透明，让顶部渐变一直铺到窗口顶端。
 * 拖动窗口、双击最大化由外层顶栏的 data-tauri-drag-region="deep" 负责。
 * 关闭按钮调用 close()，和系统关闭按钮一样触发 CloseRequested，最小化到托盘的逻辑不变。
 */
const win = getCurrentWindow();
const maximized = ref(false);
let unlisten: (() => void) | undefined;

async function syncMaximized() {
  try {
    maximized.value = await win.isMaximized();
  } catch {
    // 读不到时按未最大化显示，不影响按钮功能
  }
}

onMounted(async () => {
  await syncMaximized();
  unlisten = await win.onResized(syncMaximized);
});
onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <div class="flex h-8 items-start justify-end">
    <button type="button" class="md-winbtn" title="最小化" aria-label="最小化" @click="win.minimize()">
      <svg viewBox="0 0 10 10" class="size-2.5"><path d="M0 5.5h10" stroke="currentColor" fill="none" /></svg>
    </button>
    <button
      type="button"
      class="md-winbtn"
      :title="maximized ? '还原' : '最大化'"
      :aria-label="maximized ? '还原' : '最大化'"
      @click="win.toggleMaximize()"
    >
      <!-- 还原：两个错开的方框；最大化：一个方框 -->
      <svg v-if="maximized" viewBox="0 0 10 10" class="size-2.5">
        <path d="M2.5 0.5h7v7M0.5 2.5h7v7h-7z" stroke="currentColor" fill="none" />
      </svg>
      <svg v-else viewBox="0 0 10 10" class="size-2.5"><rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" fill="none" /></svg>
    </button>
    <button type="button" class="md-winbtn md-winbtn-close" title="关闭" aria-label="关闭" @click="win.close()">
      <svg viewBox="0 0 10 10" class="size-2.5"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" fill="none" /></svg>
    </button>
  </div>
</template>
