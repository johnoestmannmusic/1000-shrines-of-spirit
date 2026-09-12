import { test, expect, _electron as electron } from "@playwright/test";
import path from "node:path";
import os from "node:os";
import { mkdtempSync, rmSync } from "node:fs";

const projectRoot = path.resolve(__dirname, "../..");

test("New Project resets to one instrument, then add/delete instruments", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-project-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("flight_school_night_shift", {
      timeout: 30_000,
    });

    const instrumentRows = window.locator(".instrument-row");
    await expect(instrumentRows).toHaveCount(10);

    // New Project now requires confirmation.
    await window.getByRole("button", { name: "New Project" }).click();
    await expect(window.locator(".modal-title", { hasText: "New Project?" })).toBeVisible();
    await window.getByRole("button", { name: "Create New Project" }).click();
    await expect(instrumentRows).toHaveCount(1);
    await expect(window.locator(".subtitle")).toContainText("New Song");
    await expect(window.locator(".tracker .row-col").first()).toBeVisible();

    // The single remaining instrument has no enabled delete action.
    await instrumentRows.first().locator(".hamburger").click();
    await expect(instrumentRows.first().locator(".instrument-menu .menu-item")).toBeDisabled();
    await instrumentRows.first().locator(".hamburger").click();

    // Add two instruments.
    await window.getByRole("button", { name: "+ Add Instrument" }).click();
    await window.getByRole("button", { name: "+ Add Instrument" }).click();
    await expect(instrumentRows).toHaveCount(3);

    // Delete the middle instrument behind the hamburger + confirm.
    await instrumentRows.nth(1).locator(".hamburger").click();
    await instrumentRows.nth(1).getByRole("button", { name: "Delete instrument…" }).click();
    await expect(window.locator(".modal-title", { hasText: "Delete Instrument" })).toBeVisible();
    await window.getByRole("button", { name: "Delete Instrument" }).click();
    await expect(instrumentRows).toHaveCount(2);
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
