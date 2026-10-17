import { createApp } from "vue";
import App from "./App.vue";
import { applyPalette, cachedPalette } from "./palette";
import "./style.css";

// 先按上次的配色方案着色，设置读回来后 App 会再同步一次
applyPalette(cachedPalette());

createApp(App).mount("#app");
