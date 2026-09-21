import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    // Don't watch Rust build outputs: while cargo links termix_lib.dll,
    // chokidar trips EBUSY on Windows and kills the dev server
    // (then tauri reports "beforeDevCommand terminated with non-zero status").
    watch: {
      ignored: ["**/src-tauri/target/**", "**/dist/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "chrome105",
    minify: "esbuild",
    sourcemap: false,
  },
});
