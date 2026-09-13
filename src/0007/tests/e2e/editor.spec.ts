import { test, expect, _electron as electron } from "@playwright/test";
import path from "node:path";
import os from "node:os";
import { mkdtempSync, rmSync } from "node:fs";

const projectRoot = path.resolve(__dirname, "../..");

test("Sampler/Spectral modal shows waveforms and closes", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-editor-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
      timeout: 30_000,
    });

    // Open instrument 0's Sampler editor via its pill.
    await window.locator(".instrument-row").first().getByRole("button", { name: "Sampler" }).click();
    const modal = window.locator(".floating-window");
    await expect(modal).toBeVisible();
    await expect(modal.locator(".waveform canvas").first()).toBeVisible();
    // ADSR graph canvas (draggable points) on the Sampler tab.
    await expect(modal.locator("canvas.adsr-canvas")).toBeVisible();

    // Switch to the Spectral tab: Sample A + Result waveforms must render.
    await modal.getByRole("button", { name: "Spectral" }).click();
    await expect(modal.locator("h3", { hasText: "Result" })).toBeVisible();
    await expect(modal.locator(".waveform canvas").first()).toBeVisible();

    // The close button must actually close the window (drag capture regression).
    await modal.locator(".close-btn").click();
    await expect(window.locator(".floating-window")).toHaveCount(0);
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
