import { defineConfig } from "vite";
import { resolve } from "path";
import pkg from "./package.json" with { type: "json" };

// https://vitejs.dev/config/
export default defineConfig({
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        overlay: resolve(import.meta.dirname, "overlay.html"),
        result: resolve(import.meta.dirname, "result.html"),
        pin: resolve(import.meta.dirname, "pin.html"),
      },
    },
  },
});
