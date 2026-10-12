import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// 창마다 HTML 진입점을 따로 둔다: index.html(설정 편집기), osd.html(OSD).
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        osd: "osd.html",
      },
    },
  },

  // Tauri 개발용 설정 (tauri dev / tauri build에서만 쓴다)
  // 1. Rust 오류가 가려지지 않게
  clearScreen: false,
  // 2. Tauri는 고정 포트를 기대한다
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
      // 3. src-tauri는 감시하지 않는다
      ignored: ["**/src-tauri/**"],
    },
  },
}));
