import { test, expect, _electron as electron } from "@playwright/test";
import path from "node:path";
import os from "node:os";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";

const projectRoot = path.resolve(__dirname, "../..");

test("comments, Base Tempo, and no spurious audition error", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-features-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("flight_school_night_shift", {
      timeout: 30_000,
    });

    // Ref Pitch is no longer in the main header.
    await expect(window.locator(".app-header .ref-toggle")).toHaveCount(0);

    // Clicking tracker cells must not produce the "Cannot Audition" banner.
    await window.locator(".tracker tbody tr").first().locator("td").nth(1).click();
    await expect(window.locator(".audio-error")).toHaveCount(0);

    // Song Comments is present; in EDIT MODE it becomes editable.
    await expect(window.locator(".panel", { hasText: "SONG COMMENTS" })).toBeVisible();
    await window.getByRole("button", { name: "EDIT MODE" }).click();
    const comments = window.locator(".comments-edit");
    await expect(comments).toBeVisible();
    await comments.fill("Edited from the test");
    await expect(comments).toHaveValue("Edited from the test");

    // Timing card exposes an editable Base Tempo.
    await window.getByRole("button", { name: "Pattern Manager" }).waitFor();
    const timing = window.locator(".panel", { hasText: "TIMING" });
    await timing.locator(".collapse-header").click();
    const baseTempo = timing.locator("input").first();
    await expect(baseTempo).toBeVisible();
    await baseTempo.fill("150");
    await expect(baseTempo).toHaveValue("150");
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("Project JSON Load auto-applies and closes the modal", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-loadjson-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("flight_school_night_shift", {
      timeout: 30_000,
    });

    const project = JSON.parse(
      readFileSync(path.join(projectRoot, "assets/lmp-default-proj.json"), "utf8"),
    );
    project.songTitle = "Loaded Title";
    const tmp = path.join(userDataDir, "project.json");
    writeFileSync(tmp, JSON.stringify(project));

    await window.getByRole("button", { name: "Project JSON" }).click();
    await window.locator('.modal input[type="file"]').setInputFiles(tmp);

    // Auto-applies: modal closes and the new title is reflected.
    await expect(window.locator(".modal")).toHaveCount(0);
    await expect(window.locator(".subtitle")).toContainText("Loaded Title");
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
