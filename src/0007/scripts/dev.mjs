import { build as esbuildBuild } from "esbuild";
import { createServer } from "vite";
import { spawn } from "node:child_process";
import electronPath from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));

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

await buildNodeEntry("src/main/index.ts", "dist/main/index.cjs");
await buildNodeEntry("src/preload/index.ts", "dist/preload/index.cjs");

const server = await createServer({
  configFile: path.join(projectRoot, "vite.config.mts"),
  root: path.join(projectRoot, "src/renderer"),
});
await server.listen();
server.printUrls();

const port = server.config.server.port ?? 5273;
const url = `http://localhost:${port}`;

const child = spawn(electronPath, ["."], {
  cwd: projectRoot,
  stdio: "inherit",
  env: { ...process.env, ELECTRON_RENDERER_URL: url },
});

async function shutdown(code = 0) {
  try {
    await server.close();
  } catch {
    /* ignore */
  }
  process.exit(code);
}

child.on("exit", (code) => shutdown(code ?? 0));
process.on("SIGINT", () => {
  child.kill("SIGINT");
  shutdown(0);
});
process.on("SIGTERM", () => {
  child.kill("SIGTERM");
  shutdown(0);
});
