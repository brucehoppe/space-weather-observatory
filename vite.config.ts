import { defineConfig } from "vite";

// The desktop shell serves these files from disk; nothing is loaded remotely.
export default defineConfig({
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    target: "es2022",
    // Bundle everything: the privileged webview must not fetch code at runtime.
    assetsInlineLimit: 0,
    sourcemap: true,
    outDir: "dist",
    emptyOutDir: true,
  },
});
