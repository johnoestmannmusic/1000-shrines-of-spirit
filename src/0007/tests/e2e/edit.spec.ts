import { test, expect, _electron as electron } from "@playwright/test";
import path from "node:path";
import os from "node:os";
import { mkdtempSync, rmSync } from "node:fs";

const projectRoot = path.resolve(__dirname, "../..");

test("EDIT MODE: select cells, navigate with arrows, and edit", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-edit-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await window.waitForSelector(".tracker tbody tr");
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
      timeout: 30_000,
    });

    // Enter EDIT MODE.
    await window.getByRole("button", { name: "EDIT MODE" }).click();
    await expect(window.getByRole("button", { name: "Pattern Manager" })).toBeVisible();

    // Click the first channel's NOTE cell on row 0.
    const noteCell = window.locator(".tracker tbody tr").first().locator("td").nth(1);
    await noteCell.click();

    const selected = window.locator(".tracker-cell.selected");
    await expect(selected).toHaveCount(1);

    // Clicking a cell must not turn Follow Playhead off.
    await expect(window.getByLabel("Follow playhead")).toBeChecked();
    // A quick click is a single-cell selection, not a range (only the
    // selected cell reports in-range).
    await expect(window.locator(".tracker-cell.in-range")).toHaveCount(1);

    // Read the selection's row before/after ArrowDown.
    const selectedRowBefore = await window.evaluate(() => {
      const el = document.querySelector(".tracker-cell.selected");
      return el ? (el.closest("tr")?.textContent ?? "") : "";
    });
    await window.keyboard.press("ArrowDown");
    const selectedRowAfter = await window.evaluate(() => {
      const el = document.querySelector(".tracker-cell.selected");
      return el ? (el.closest("tr")?.textContent ?? "") : "";
    });
    expect(selectedRowAfter).not.toBe(selectedRowBefore);

    // Edit: enter a Note Off (C) into the selected NOTE cell and confirm it renders.
    await window.keyboard.press("c");
    await expect(window.locator(".tracker-cell.selected")).toContainText(/OFF/);
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("EDIT MODE scrolls the selected cell into view", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-scroll-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
      timeout: 30_000,
    });
    await window.getByRole("button", { name: "EDIT MODE" }).click();
    await window.locator(".tracker tbody tr").first().locator("td").nth(1).click();

    // Move well past the ~24 visible rows.
    for (let i = 0; i < 45; i++) await window.keyboard.press("ArrowDown");

    const box = await window.evaluate(() => {
      const selected = document.querySelector(".tracker-cell.selected") as HTMLElement | null;
      const container = document.querySelector(".tracker") as HTMLElement | null;
      if (!selected || !container) return null;
      const s = selected.getBoundingClientRect();
      const c = container.getBoundingClientRect();
      return { selTop: s.top, selBottom: s.bottom, cTop: c.top, cBottom: c.bottom };
    });
    expect(box).not.toBeNull();
    // The selected cell must be inside the scroll viewport (below the sticky header).
    expect(box!.selTop).toBeGreaterThanOrEqual(box!.cTop + 40);
    expect(box!.selBottom).toBeLessThanOrEqual(box!.cBottom + 1);
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("EDIT MODE: Z repeats the last entered note", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-z-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });
    await window.getByRole("button", { name: "EDIT MODE" }).click();

    // Enter a note via the right-click menu on row 0, channel 0.
    const firstNote = window.locator(".tracker tbody tr").first().locator("td").nth(1);
    await firstNote.click({ button: "right" });
    await window
      .locator(".context-menu")
      .getByRole("button", { name: "D-4", exact: true })
      .click();

    // Adjust it up one semitone (Q), then Z must repeat the adjusted value
    // (D#4) exactly, not one step further.
    await window.keyboard.press("q");
    await expect(firstNote).toHaveText("D#4");

    const secondNote = window.locator(".tracker tbody tr").nth(2).locator("td").nth(1);
    await secondNote.click();
    await window.keyboard.press("z");
    await expect(
      window.locator(".tracker tbody tr").nth(2).locator("td").nth(1),
    ).toHaveText("D#4");
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("EDIT MODE: hold-to-drag selects a range", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-drag-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });
    await window.getByRole("button", { name: "EDIT MODE" }).click();

    const start = window.locator(".tracker tbody tr").nth(8).locator("td").nth(1);
    const end = window.locator(".tracker tbody tr").nth(16).locator("td").nth(1);
    const startBox = await start.boundingBox();
    const endBox = await end.boundingBox();
    if (!startBox || !endBox) throw new Error("cell not visible");

    await window.mouse.move(startBox.x + startBox.width / 2, startBox.y + startBox.height / 2);
    await window.mouse.down();
    await window.waitForTimeout(320); // exceed the 250ms hold threshold
    await window.mouse.move(endBox.x + endBox.width / 2, endBox.y + endBox.height / 2, {
      steps: 6,
    });
    await window.mouse.up();

    await expect(window.locator(".tracker-cell.in-range").first()).toBeVisible();
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
