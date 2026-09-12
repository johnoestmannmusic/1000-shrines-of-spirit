import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/web",
  timeout: 60_000,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:8123", channel: "chrome" },
  webServer: {
    command: "python3 -m http.server 8123 --directory dist/web",
    url: "http://127.0.0.1:8123",
    reuseExistingServer: true,
    timeout: 30_000,
  },
});
