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
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
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
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
      timeout: 30_000,
    });

    const project = JSON.parse(
      readFileSync(path.join(projectRoot, "assets/lmp-default-proj.lampjson"), "utf8"),
    );
    project.songTitle = "Loaded Title";
    const tmp = path.join(userDataDir, "project.lampjson");
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

test("Project JSON Load rejects non-.lampjson files", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-rejectjson-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", {
      timeout: 30_000,
    });

    const project = JSON.parse(
      readFileSync(path.join(projectRoot, "assets/lmp-default-proj.lampjson"), "utf8"),
    );
    const tmp = path.join(userDataDir, "project.json");
    writeFileSync(tmp, JSON.stringify(project));

    await window.getByRole("button", { name: "Project JSON" }).click();
    await window.locator('.modal input[type="file"]').setInputFiles(tmp);

    await expect(window.locator(".error-banner")).toContainText(".lampjson");
    // The modal stays open because the file was rejected.
    await expect(window.locator(".modal")).toHaveCount(1);
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("all six Source Samples load", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-samples-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });
    await expect(
      window.locator(".panel", { hasText: "SOURCE SAMPLES" }).getByText("6 / 6"),
    ).toBeVisible({ timeout: 30_000 });
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("instrument vibrato fields can be cleared and retyped", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-vib-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    const depth = window.locator(".instrument-row").first().getByLabel("Depth");
    await depth.click();
    await window.keyboard.press("Control+a");
    await window.keyboard.press("Backspace");
    await expect(depth).toHaveValue("");

    await depth.type("1.5");
    await window.keyboard.press("Enter");
    await expect(depth).toHaveValue("1.5");
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("instrument names save into Project JSON", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-names-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    const nameInput = window.locator(".instrument-row").first().locator(".ins-name-input");
    await nameInput.fill("Test Bass");
    await window.getByRole("button", { name: "Project JSON" }).click();
    await expect(window.locator(".project-json")).toHaveValue(/Test Bass/, { timeout: 5_000 });
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("Master FX modal opens with Delay and Reverb", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-masterfx-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    await window.getByRole("button", { name: "Master FX" }).click();
    await expect(window.locator(".modal-title", { hasText: "Master FX" })).toBeVisible();
    await expect(window.locator(".modal", { hasText: "Delay" })).toBeVisible();
    await expect(window.locator(".modal", { hasText: "Reverb" })).toBeVisible();
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("loading Project JSON restores instrument names", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-loadnames-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    const project = JSON.parse(
      readFileSync(path.join(projectRoot, "assets/lmp-default-proj.lampjson"), "utf8"),
    );
    project.instrumentNames = ["Loaded Bass"];
    const tmp = path.join(userDataDir, "names.lampjson");
    writeFileSync(tmp, JSON.stringify(project));

    await window.getByRole("button", { name: "Project JSON" }).click();
    await window.locator('.modal input[type="file"]').setInputFiles(tmp);

    await expect(window.locator(".modal")).toHaveCount(0);
    await expect(
      window.locator(".instrument-row").first().locator(".ins-name-input"),
    ).toHaveValue("Loaded Bass");
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("Master FX settings save into and load from Project JSON", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-masterfxio-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    // Enable Delay in Master FX, then confirm it appears in the exported JSON.
    await window.getByRole("button", { name: "Master FX" }).click();
    const fxModal = window.locator(".modal", { hasText: "Master FX" });
    await fxModal.getByRole("checkbox").first().check();
    await fxModal.locator(".close-btn").click();

    await window.getByRole("button", { name: "Project JSON" }).click();
    await expect(window.locator(".project-json")).toHaveValue(/"masterFx"/);
    await expect(window.locator(".project-json")).toHaveValue(/"enabled": true/);
    await window.locator(".modal-title button").click();

    // Now load a project whose Delay is enabled and confirm the modal reflects it.
    const project = JSON.parse(
      readFileSync(path.join(projectRoot, "assets/lmp-default-proj.lampjson"), "utf8"),
    );
    project.masterFx = project.masterFx ?? {
      delay: { enabled: false, timeSec: 0.25, feedback: 0.45, toneHz: 2600, mix: 0.35 },
      reverb: { enabled: false, decaySec: 2, mix: 0.25 },
    };
    project.masterFx.delay.enabled = true;
    project.masterFx.reverb.enabled = true;
    const tmp = path.join(userDataDir, "fx.lampjson");
    writeFileSync(tmp, JSON.stringify(project));

    await window.getByRole("button", { name: "Project JSON" }).click();
    await window.locator('.modal input[type="file"]').setInputFiles(tmp);
    await expect(window.locator(".modal")).toHaveCount(0);

    await window.getByRole("button", { name: "Master FX" }).click();
    const loaded = window.locator(".modal", { hasText: "Master FX" });
    await expect(loaded.getByRole("checkbox").nth(0)).toBeChecked();
    await expect(loaded.getByRole("checkbox").nth(1)).toBeChecked();
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("Save WAV opens the export modal with loops, fades and normalize", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-wavmodal-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    await window.getByRole("button", { name: "Save .WAV" }).click();
    const modal = window.locator(".modal", { hasText: "Export WAV" });
    await expect(modal).toBeVisible();
    await expect(modal.getByLabel("Number of Loops")).toHaveValue("0");
    await expect(modal.getByLabel("Fade In (ms)")).toHaveValue("0");
    await expect(modal.getByLabel("Fade Out (ms)")).toHaveValue("0");
    await expect(modal.getByRole("checkbox")).toBeChecked();
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("license URLs render as external links", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-links-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    const licenses = window.locator(".panel", { hasText: "LICENSES" });
    const link = licenses.locator(".linkified a").first();
    await expect(link).toHaveAttribute("href", /^https:\/\//);
    await expect(licenses.locator(".linkified").first()).toHaveCSS("word-break", "break-word");
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("Mixer rows fit inside the panel", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-mixer-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    const rows = await window.evaluate(() =>
      Array.from(document.querySelectorAll(".mixer-row")).map((row) => ({
        rowRight: row.getBoundingClientRect().right,
        childRight: Math.max(
          ...Array.from(row.children).map((child) => child.getBoundingClientRect().right),
        ),
      })),
    );
    expect(rows.length).toBe(5);
    for (const { rowRight, childRight } of rows) {
      expect(childRight).toBeLessThanOrEqual(rowRight + 1);
    }
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});

test("dragging pan during playback keeps the UI responsive", async () => {
  const userDataDir = mkdtempSync(path.join(os.tmpdir(), "lantern-pandrag-"));
  const app = await electron.launch({
    args: [projectRoot, "--no-sandbox", `--user-data-dir=${userDataDir}`],
    cwd: projectRoot,
  });
  try {
    const window = await app.firstWindow();
    await expect(window.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

    const play = window.getByRole("button", { name: /Play/ });
    await expect(play).toBeEnabled({ timeout: 30_000 });
    await play.click();

    // Stress the pan control (the first range in the row) the way a drag would.
    const slider = window.locator(".instrument-row").first().locator('input[type="range"]').first();
    await slider.focus();
    for (let i = 0; i < 60; i++) {
      await window.keyboard.press(i % 2 === 0 ? "ArrowRight" : "ArrowLeft");
    }

    // The app must still respond promptly afterwards.
    await window.getByRole("button", { name: "EDIT MODE" }).click();
    await expect(window.getByRole("button", { name: "Pattern Manager" })).toBeVisible({
      timeout: 5_000,
    });
  } finally {
    await app.close();
    rmSync(userDataDir, { recursive: true, force: true });
  }
});
