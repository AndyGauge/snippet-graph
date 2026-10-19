import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: "../static/islands",
    emptyOutDir: true,
    cssCodeSplit: false,
    rollupOptions: {
      input: "src/main.js",
      output: {
        entryFileNames: "islands.js",
        chunkFileNames: "[name].js",
        assetFileNames: "islands[extname]",
      },
    },
  },
});
