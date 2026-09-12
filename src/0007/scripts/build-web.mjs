import { build as viteBuild } from "vite";
import { cp, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const outDir = path.join(projectRoot, "dist/web");

await rm(outDir, { recursive: true, force: true });

await viteBuild({
  configFile: path.join(projectRoot, "vite.config.mts"),
  root: path.join(projectRoot, "src/renderer"),
  build: { outDir, emptyOutDir: true },
});

// Serve the bundled song alongside the app. The large CHIP mix WAV is skipped
// to keep the web bundle light — CHIP mode still plays via the four stems, but
// its "Save .WAV" passthrough needs the original mix, so include it if wanted.
await cp(path.join(projectRoot, "assets"), path.join(outDir, "assets"), {
  recursive: true,
  filter: (src) => !src.endsWith("flight_school_night_shift.wav"),
});

console.log("Web build written to dist/web/");
console.log("Serve it with a static server, e.g.:");
console.log("  npx serve dist/web");
console.log("  # or: python3 -m http.server 8000 --directory dist/web");
