// Builds the window's page into dist, which the Tauri shell opens.
import { join } from "node:path";

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const src = join(import.meta.dirname, "src");

export default defineConfig({
  base: "./", // The page is served from the app's own files, so everything it loads is named relative to it.
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": src } },
  // The page is read from the app, never downloaded, so one bundle is right and Vite's warning about its size does not apply.
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false, chunkSizeWarningLimit: 2000 },
  test: { include: ["src/**/*.test.ts", "src/**/*.test.tsx"] }
});
