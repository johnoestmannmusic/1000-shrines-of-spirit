import { test, expect, _electron as electron } from "@playwright/test";
import path from "node:path";
import os from "node:os";
import { mkdtempSync, rmSync } from "node:fs";

const projectRoot = path.resolve(__dirname, "../..");

test("loads the bundled song and renders the player shell", async () => {
  // Isolate userData so persisted theme/settings from earlier runs can't leak in.
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-e2e-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });

  try {
    const window = await app.firstWindow();
    await window.waitForSelector(".app-header h1");

    await expect(window.locator(".app-header h1")).toContainText("Lantern Music Player");
    // The heading carries a build-stamped date: vYYYYMMDD.
    await expect(window.locator(".app-header h1")).toHaveText(/Lantern Music Player v\d{8}$/);
    // The status line is set only after the .fur parses and the project loads.
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
      timeout: 30_000,
    });

    // Transport, mixer and tracker all render.
    await expect(window.getByRole("button", { name: /Play/ })).toBeVisible();
    await expect(window.locator("h2", { hasText: "MIXER" })).toBeVisible();
    await expect(window.locator(".tracker")).toBeVisible();
    await expect(window.locator(".tracker tbody tr").first()).toBeVisible();
    // Not in CHIP MODE, so channel headings are numbered only (no roles).
    await expect(window.locator(".tracker thead")).toContainText("CH0");
    await expect(window.locator(".tracker thead")).not.toContainText("PULSE 1");

    // Branding: Light is the default; the toggle switches themes.
    await expect(window.locator("html")).toHaveAttribute("data-theme", "light");
    await window.getByRole("button", { name: /Light/ }).click();
    await expect(window.locator("html")).toHaveAttribute("data-theme", "dark");

    // The Project JSON window opens with the version-1 schema.
    await window.getByRole("button", { name: "Project JSON" }).click();
    await expect(window.locator(".project-json")).toContainText('"version": 1');
    await window.locator(".modal-title button").click();
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
