import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  outputDir: "../tmp/playwright-results",
  reporter: "list",
  use: { trace: "retain-on-failure" }
});
