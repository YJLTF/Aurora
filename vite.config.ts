import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  // Tauri 需要固定端口，且不要清理终端输出
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  // 桌面端运行时目标为 webview，不用做 esnext 全量转译
  build: {
    target: "chrome105",
    minify: true,
  },
});
