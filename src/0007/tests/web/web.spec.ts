import { test, expect } from "@playwright/test";

test("web build loads the bundled song in a browser", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  const notFound: string[] = [];
  page.on("response", (response) => {
    if (response.status() >= 400) notFound.push(`${response.status()} ${response.url()}`);
  });

  await page.goto("/");
  await expect(page.locator(".app-header h1")).toContainText("Lantern Music Player");
  await expect(page.locator(".toolbar .status")).toContainText("instruments", {
    timeout: 30_000,
  });
  await expect(page.locator(".tracker tbody tr").first()).toBeVisible();
  // CHIP-mode assets (stems / .fur / mix WAV) are optional and absent for
  // this project-only song, so their 404s are expected.
  const optional = /\/assets\/([0-3]\.ogg|flight_school_night_shift\.(fur|wav))$/;
  expect(notFound.filter((url) => !optional.test(url))).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("WAV export works when OfflineAudioContext.suspend is unavailable", async ({ page }) => {
  await page.addInitScript(() => {
    const ctx = (window as unknown as { OfflineAudioContext?: { prototype: object } })
      .OfflineAudioContext;
    if (ctx) {
      Object.defineProperty(ctx.prototype, "suspend", { value: undefined, configurable: true });
      Object.defineProperty(ctx.prototype, "resume", { value: undefined, configurable: true });
    }
  });

  await page.goto("/");
  await expect(page.locator(".toolbar .status")).toContainText("instruments", { timeout: 30_000 });

  // Enable Delay so the offline FX render (and its progress path) actually runs.
  await page.getByRole("button", { name: "Master FX" }).click();
  const fx = page.locator(".modal", { hasText: "Master FX" });
  await fx.getByRole("checkbox").first().check();
  await fx.locator(".close-btn").click();

  await page.getByRole("button", { name: "Save .WAV" }).click();
  const modal = page.locator(".modal", { hasText: "Export WAV" });
  await modal.getByRole("checkbox").uncheck();
  const downloadPromise = page.waitForEvent("download", { timeout: 120_000 });
  await modal.getByRole("button", { name: "Export WAV" }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toMatch(/\.wav$/);
  await expect(page.locator(".error-banner")).toHaveCount(0);
});
