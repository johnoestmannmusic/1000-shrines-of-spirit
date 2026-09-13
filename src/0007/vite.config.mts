import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("./src/renderer", import.meta.url));
const outDir = fileURLToPath(new URL("./dist/renderer", import.meta.url));
const src = fileURLToPath(new URL("./src", import.meta.url));

const buildDate = (() => {
  const d = new Date();
  return `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
})();

export default defineConfig({
  define: { __BUILD_DATE__: JSON.stringify(buildDate) },
  root,
  base: "./",
  plugins: [react()],
  resolve: {
    alias: { "@": src },
  },
  build: {
    outDir,
    emptyOutDir: true,
    target: "chrome120",
  },
  server: {
    port: 5273,
    strictPort: true,
  },
});
