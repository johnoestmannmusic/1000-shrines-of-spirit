import { build as esbuildBuild } from "esbuild";
import { build as viteBuild } from "vite";
import { rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const dist = path.join(projectRoot, "dist");

async function buildNodeEntry(relEntry, relOutfile) {
  await esbuildBuild({
    entryPoints: [path.join(projectRoot, relEntry)],
    bundle: true,
    platform: "node",
    format: "cjs",
    target: "node22",
    sourcemap: true,
    external: ["electron"],
    outfile: path.join(projectRoot, relOutfile),
  });
}

await rm(dist, { recursive: true, force: true });

await buildNodeEntry("src/main/index.ts", "dist/main/index.cjs");
await buildNodeEntry("src/preload/index.ts", "dist/preload/index.cjs");

await viteBuild({
  configFile: path.join(projectRoot, "vite.config.mts"),
  root: path.join(projectRoot, "src/renderer"),
});

console.log("Build complete: dist/");
