import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { codeInspectorPlugin } from "code-inspector-plugin";

function rendererVendorChunk(id: string): string | undefined {
  if (!id.includes("/node_modules/")) return undefined;

  if (
    id.includes("/node_modules/react/") ||
    id.includes("/node_modules/react-dom/") ||
    id.includes("/node_modules/react-i18next/") ||
    id.includes("/node_modules/scheduler/")
  ) {
    return "vendor-react";
  }
  if (id.includes("/node_modules/@tanstack/")) return "vendor-query";
  if (id.includes("/node_modules/i18next/")) return "vendor-i18n";
  if (id.includes("/node_modules/zod/")) return "vendor-validation";
  if (id.includes("/node_modules/@tauri-apps/")) return "vendor-tauri";
  if (id.includes("/node_modules/sonner/")) return "vendor-notifications";
  if (
    id.includes("/node_modules/tailwind-merge/") ||
    id.includes("/node_modules/clsx/") ||
    id.includes("/node_modules/class-variance-authority/") ||
    id.includes("/node_modules/@radix-ui/react-slot/") ||
    id.includes("/node_modules/@radix-ui/react-compose-refs/")
  ) {
    return "vendor-ui-core";
  }

  return undefined;
}

export default defineConfig(({ command }) => ({
  root: "src",
  plugins: [
    command === "serve" &&
      codeInspectorPlugin({
        bundler: "vite",
      }),
    react(),
  ].filter(Boolean),
  base: "./",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        // Stable framework chunks keep route code small and let WebView reuse
        // unchanged runtime assets across app updates. Packages not listed here
        // stay with their owning lazy route so this never becomes a catch-all
        // vendor bundle that defeats route-level loading.
        manualChunks: rendererVendorChunk,
      },
    },
  },
  server: {
    port: 3000,
    strictPort: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  // `TAURI_ENV_*` rather than a bare `TAURI_` prefix: the release build runs
  // with `TAURI_SIGNING_PRIVATE_KEY` and its password in the environment, and a
  // broad prefix puts both within reach of anything that reads `import.meta.env`.
  // The `TAURI_ENV_*` set the CLI injects is platform metadata and carries no
  // secrets. This matches the prefix pair in Tauri's own Vite template.
  envPrefix: ["VITE_", "TAURI_ENV_*"],
}));
