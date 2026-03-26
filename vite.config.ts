import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { copyFileSync, mkdirSync } from "fs";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;
const isTauri = !!process.env.TAURI_ENV_PLATFORM || !!host;

export default defineConfig(async () => ({
  resolve: isTauri ? {} : {
    alias: {
      "@tauri-apps/api/core": resolve("src/lib/tauri-mock.ts"),
      "@tauri-apps/api/webview": resolve("src/lib/tauri-mock-webview.ts"),
      "@tauri-apps/plugin-dialog": resolve("src/lib/tauri-mock-dialog.ts"),
    },
  },
  plugins: [
    react(),
    {
      name: "copy-pdfjs-worker",
      buildStart() {
        // Copy PDF.js worker to public so it's always available
        const workerSrc = resolve("node_modules/pdfjs-dist/build/pdf.worker.min.mjs");
        const publicDir = resolve("public");
        mkdirSync(publicDir, { recursive: true });
        copyFileSync(workerSrc, resolve(publicDir, "pdf.worker.min.mjs"));
      },
    },
  ],
  clearScreen: false,
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
      ignored: ["**/src-tauri/**"],
    },
  },
}));
