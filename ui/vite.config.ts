// Builds the window's page into dist, which the Tauri shell opens.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

const src = join(import.meta.dirname, "src");

// Two dependencies write a <style> element as they run, which the page's content security policy refuses: Sonner its whole stylesheet as it loads, and Radix's scroll area a rule that hides the native scroll bars.
// globals.css loads the same CSS from a file, and this takes the two writes out as they are bundled. The build stops if one is no longer there to take out, so a new version is looked at before it meets the policy.
const STYLE_ELEMENTS: readonly { readonly file: string; readonly write: RegExp }[] = [
  { file: "/node_modules/sonner/dist/", write: /^__insertCSS\(".*"\);$/m },
  { file: "/node_modules/@radix-ui/react-scroll-area/dist/", write: /\/\* @__PURE__ \*\/ jsx\(ScrollAreaViewportStyle, \{ nonce \}\),/ }
];
const noStyleElements: Plugin = {
  name: "no-style-elements",
  transform(code, id) {
    const found = STYLE_ELEMENTS.find((each) => id.includes(each.file));
    if (found === undefined) {
      return null;
    }
    if (!found.write.test(code)) {
      throw new Error(`${id} no longer writes its <style> element the way vite.config.ts expects: look at how it does now, before it meets the content security policy`);
    }
    return { code: code.replace(found.write, ""), map: null };
  }
};

const manifest: unknown = JSON.parse(readFileSync(join(import.meta.dirname, "package.json"), "utf8"));
const version = typeof manifest === "object" && manifest !== null && "version" in manifest ? manifest.version : "";

export default defineConfig({
  // The version shown when the hub's differs; package.json takes it from Cargo.toml (task P3-3).
  define: { __APP_VERSION__: JSON.stringify(typeof version === "string" ? version : "") },
  base: "./", // The page is served from the app's own files, so everything it loads is named relative to it.
  plugins: [react(), tailwindcss(), noStyleElements],
  // react-style-singleton writes <style> elements, which the page's content security policy refuses; see styleSingleton.ts.
  resolve: { alias: { "@": src, "react-style-singleton": join(src, "styleSingleton.ts") } },
  // The page is read from the app, never downloaded, so one bundle is right and Vite's warning about its size does not apply.
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false, chunkSizeWarningLimit: 2000 },
  test: { include: ["src/**/*.test.ts", "src/**/*.test.tsx"] }
});
