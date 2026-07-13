import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFile, execFileSync } from "node:child_process";
import { chmod, cp, mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const root = path.resolve(import.meta.dirname, "../..");
const settingsRoot = path.join(root, "settings");
const cli = path.join(root, "target/debug/codex-voice");
const runRoot = path.join(root, "tmp/e2e-core-workflows");

let app: ElectronApplication;
let page: Page;
let environment: NodeJS.ProcessEnv;

async function executable(file: string, contents: string) {
  await writeFile(file, contents);
  await chmod(file, 0o755);
}

async function runCli(...args: string[]) {
  return execFileAsync(cli, args, { cwd: root, env: environment });
}

async function settings() {
  return JSON.parse((await runCli("settings", "get")).stdout) as {
    enabled: boolean; showTrayIcon: boolean; keybinding: string; language: string;
  };
}

test.beforeAll(async () => {
  await rm(runRoot, { recursive: true, force: true });
  const schema = path.join(runRoot, "schemas");
  const bin = path.join(runRoot, "bin");
  const runtime = path.join(runRoot, "runtime");
  await Promise.all([schema, bin, runtime, path.join(runRoot, "config"), path.join(runRoot, "data")].map(directory => mkdir(directory, { recursive: true })));
  await cp(path.join(root, "schemas/io.github.andy_spike.CodexVoice.gschema.xml"), path.join(schema, "io.github.andy_spike.CodexVoice.gschema.xml"));
  execFileSync("glib-compile-schemas", [schema]);

  await executable(path.join(bin, "arecord"), `#!/usr/bin/env node
const fs = require("node:fs");
const output = process.argv.at(-1);
fs.writeFileSync(output, Buffer.alloc(256, 1));
const done = () => process.exit(0);
process.on("SIGINT", done); process.on("SIGTERM", done); setInterval(() => {}, 1000);
`);
  await executable(path.join(bin, "codex-asr"), `#!/usr/bin/env node
process.stdout.write("End to end dictated text\\n");
`);
  await executable(path.join(bin, "ydotool"), `#!/usr/bin/env node
require("node:fs").writeFileSync(process.env.E2E_TYPED_LOG, process.argv.slice(2).join(" "));
`);
  const overlay = path.join(runRoot, "overlay.py");
  await executable(overlay, `#!/usr/bin/env python3
import signal, time
running = True
def stop(*_):
    global running
    running = False
signal.signal(signal.SIGINT, stop)
signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGUSR1, lambda *_: None)
while running: time.sleep(0.05)
`);

  environment = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH}`,
    GSETTINGS_SCHEMA_DIR: schema,
    GSETTINGS_BACKEND: "keyfile",
    XDG_CONFIG_HOME: path.join(runRoot, "config"),
    XDG_DATA_HOME: path.join(runRoot, "data"),
    XDG_RUNTIME_DIR: runtime,
    CODEX_VOICE_BIN: cli,
    CODEX_VOICE_OVERLAY: overlay,
    E2E_TYPED_LOG: path.join(runRoot, "typed.txt")
  };

  app = await electron.launch({
    args: [".", "--no-sandbox", "--ozone-platform=x11", `--user-data-dir=${path.join(runRoot, "electron")}`],
    cwd: settingsRoot,
    env: environment
  });
  page = await app.firstWindow();
  await expect(page.getByRole("heading", { name: "Codex Voice" })).toBeVisible();
});

test.afterAll(async () => {
  await runCli("--cancel").catch(() => undefined);
  await runCli("--close-preview").catch(() => undefined);
  await app?.close();
});

test("settings persist, synchronize, validate shortcuts, and reset", async () => {
  const enabled = page.getByRole("checkbox", { name: "Dictation enabled" });
  const tray = page.getByRole("checkbox", { name: "Show top-bar icon" });
  await expect(enabled).toBeChecked();
  await enabled.uncheck();
  await expect.poll(async () => (await settings()).enabled).toBe(false);
  await tray.uncheck();
  await expect.poll(async () => (await settings()).showTrayIcon).toBe(false);

  await runCli("settings", "set", "enabled", "true");
  await expect(enabled).toBeChecked();

  const shortcut = page.getByRole("button", { name: /Ctrl.*Super.*Space/i });
  await shortcut.click();
  await page.keyboard.press("Control+Alt+KeyO");
  await expect.poll(async () => (await settings()).keybinding).toBe("<Control><Alt>o");
  await page.getByRole("button", { name: /Ctrl.*Alt.*o/i }).click();
  await page.evaluate(() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Unidentified", code: "Space", ctrlKey: true, metaKey: true, bubbles: true })));
  await expect.poll(async () => (await settings()).keybinding).toBe("<Control><Super>space");
  await expect(page.getByRole("alert")).toHaveCount(0);

  await page.getByRole("combobox", { name: "Language" }).click();
  await page.getByRole("option", { name: "Spanish (Mexico)" }).click();
  await expect.poll(async () => (await settings()).language).toBe("es-mx");

  await page.getByRole("button", { name: "Advanced" }).click();
  await page.getByRole("button", { name: "Reset settings" }).click();
  const reset = page.getByRole("dialog", { name: "Reset all settings?" });
  await reset.getByRole("button", { name: "Reset settings" }).click();
  await expect.poll(settings).toMatchObject({ enabled: true, showTrayIcon: true, keybinding: "<Control><Super>space", language: "auto" });
});

test("dictation honors enabled state and completes recording, transcription, typing, and history", async () => {
  await runCli("settings", "set", "enabled", "false");
  await expect(runCli("--start")).rejects.toThrow(/dictation is paused/);
  await runCli("settings", "set", "enabled", "true");
  await runCli("--start");
  await expect.poll(async () => JSON.parse((await runCli("--status")).stdout).state).toBe("recording");
  const stopped = await runCli("--stop");
  expect(stopped.stdout.trim()).toBe("End to end dictated text");
  await expect.poll(async () => JSON.parse((await runCli("--status")).stdout).state).toBe("idle");

  await page.getByRole("tab", { name: "Transcriptions" }).click();
  await expect(page.getByText("End to end dictated text")).toBeVisible();
  await page.getByRole("searchbox", { name: "Search transcript history" }).fill("dictated");
  await expect(page.getByText("End to end dictated text")).toBeVisible();
  await page.getByRole("button", { name: "Copy" }).click();
  await expect(page.getByRole("button", { name: "Copied" })).toBeVisible();
  await page.getByRole("button", { name: /Delete transcript from/ }).click();
  await expect(page.getByText("End to end dictated text")).toHaveCount(0);
});

test("recording pill preview opens once, closes, and can be reopened", async () => {
  await page.getByRole("tab", { name: "General" }).click();
  const show = page.getByRole("button", { name: "Show live preview" });
  await show.click();
  await expect(page.getByRole("button", { name: "Close preview" })).toBeEnabled();
  await page.getByRole("button", { name: "Close preview" }).click();
  await expect(show).toBeEnabled();
  await show.click();
  await expect(page.getByRole("button", { name: "Close preview" })).toBeEnabled();
  await page.getByRole("button", { name: "Close preview" }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
});
