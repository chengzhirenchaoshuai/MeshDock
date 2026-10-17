import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
// @ts-expect-error 未安装 @types/node 时的类型报错
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [vue(), tailwindcss()],

  // 以下为 Tauri 开发所需的 Vite 选项
  // 1. 不清屏，避免遮住 Rust 编译错误
  clearScreen: false,
  // 2. Tauri 需要固定端口，端口被占用时直接失败
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. 不监听 src-tauri 目录
      ignored: ["**/src-tauri/**"],
    },
  },
}));
