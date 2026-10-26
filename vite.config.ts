import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// TAURI_DEV_HOST is set by `tauri dev` when targeting a separate device/host
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
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
      // flatpak/build-dir holds a copied udev watch tree with symlink loops,
      // which crashes the file watcher (ELOOP).
      ignored: [
        "**/src-tauri/**",
        "**/flatpak/**",
        "**/.flatpak-builder/**",
        "**/snap/**",
        "**/target/**",
      ],
    },
  },
});
