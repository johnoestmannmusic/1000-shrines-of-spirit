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
  await expect(page.locator(".toolbar .status")).toContainText("flight_school_night_shift", {
    timeout: 30_000,
  });
  await expect(page.locator(".tracker tbody tr").first()).toBeVisible();
  expect(notFound).toEqual([]);
  expect(consoleErrors).toEqual([]);
});
