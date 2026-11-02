import { defineConfig } from "vitest/config";

// Tauri expects a fixed dev-server port (see src-tauri/tauri.conf.json).
// https://v2.tauri.app/start/frontend/vite/
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/agent/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // WebView2 (Windows) is Chromium; WebKit on macOS/Linux supports Safari 13+.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
