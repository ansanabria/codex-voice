import { app, BrowserWindow, ipcMain, Menu, nativeTheme } from "electron";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { promisify } from "node:util";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { BOOLEAN_SETTINGS_KEYS, SETTINGS_KEYS, parseSettings, parseStatus, type AppInfo, type SettingsDocument, type SettingsKey } from "./contract.js";

const execFileAsync = promisify(execFile);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const validKeys: ReadonlySet<string> = new Set(SETTINGS_KEYS);
let window: BrowserWindow | null = null;
let settingsMonitor: ChildProcess | null = null;
let previewProcess: ChildProcess | null = null;
let monitorReloadTimer: NodeJS.Timeout | null = null;
let monitorReloadRunning = false;
let monitorReloadQueued = false;
let lastSettingsJson = "";
let closingPreviewForQuit = false;
let quitAfterPreviewCleanup = false;

function windowState() {
  return { maximized: window?.isMaximized() ?? false };
}

function broadcastWindowState() {
  window?.webContents.send("codex-voice:window-state", windowState());
}

function broadcastPreviewClosed() {
  window?.webContents.send("codex-voice:preview-closed");
}

async function showPreview() {
  if (previewProcess && previewProcess.exitCode === null) return;
  const child = spawn(cli(), ["--preview"], { stdio: "ignore", windowsHide: true });
  previewProcess = child;
  child.once("exit", () => {
    if (previewProcess !== child) return;
    previewProcess = null;
    broadcastPreviewClosed();
  });
  await new Promise<void>((resolve, reject) => {
    child.once("spawn", resolve);
    child.once("error", error => {
      if (previewProcess === child) previewProcess = null;
      reject(error);
    });
  });
}

async function closePreview() {
  await execFileAsync(cli(), ["--close-preview"], {
    shell: false,
    windowsHide: true,
    timeout: 5000
  });
}

function windowBackground() {
  return nativeTheme.shouldUseDarkColors ? "#0F0F0F" : "#FAFAFA";
}

function cli() {
  return process.env.CODEX_VOICE_BIN || "codex-voice";
}

async function runSettings(args: string[]): Promise<SettingsDocument> {
  const { stdout } = await execFileAsync(cli(), ["settings", ...args], {
    shell: false,
    windowsHide: true,
    maxBuffer: 1024 * 1024
  });
  return parseSettings(stdout);
}

function rememberSettings(settings: SettingsDocument) {
  lastSettingsJson = JSON.stringify(settings);
  return settings;
}

async function broadcastSettings() {
  if (monitorReloadRunning) {
    monitorReloadQueued = true;
    return;
  }
  monitorReloadRunning = true;
  try {
    const settings = await runSettings(["get"]);
    const serialized = JSON.stringify(settings);
    if (serialized !== lastSettingsJson) {
      lastSettingsJson = serialized;
      window?.webContents.send("codex-voice:settings-changed", settings);
    }
  } catch (error) {
    console.error("Could not reload changed settings", error);
  } finally {
    monitorReloadRunning = false;
    if (monitorReloadQueued) {
      monitorReloadQueued = false;
      void broadcastSettings();
    }
  }
}

function scheduleSettingsReload() {
  if (monitorReloadTimer) clearTimeout(monitorReloadTimer);
  monitorReloadTimer = setTimeout(() => {
    monitorReloadTimer = null;
    void broadcastSettings();
  }, 50);
}

function startSettingsMonitor() {
  if (settingsMonitor) return;
  const schemaDirectories = [
    path.join(os.homedir(), ".local/share/codex-voice/schemas"),
    "/usr/share/glib-2.0/schemas",
    ...(process.env.GSETTINGS_SCHEMA_DIR?.split(path.delimiter) ?? [])
  ].filter(directory => directory && existsSync(directory));
  const environment = {
    ...process.env,
    ...(schemaDirectories.length > 0
      ? { GSETTINGS_SCHEMA_DIR: [...new Set(schemaDirectories)].join(path.delimiter) }
      : {})
  };
  const monitor = spawn("gsettings", ["monitor", "io.github.andy_spike.CodexVoice"], {
    env: environment,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true
  });
  settingsMonitor = monitor;
  monitor.stdout.on("data", scheduleSettingsReload);
  monitor.stderr.on("data", data => console.error(`gsettings monitor: ${String(data).trim()}`));
  monitor.on("error", error => console.error("Could not monitor settings", error));
  monitor.on("exit", () => { if (settingsMonitor === monitor) settingsMonitor = null; });
}

function isSettingsKey(value: unknown): value is SettingsKey {
  return typeof value === "string" && validKeys.has(value);
}

function validateUpdate(key: unknown, value: unknown): asserts key is SettingsKey {
  if (!isSettingsKey(key)) throw new Error("Unsupported settings key");
  if (typeof value !== "string" && typeof value !== "boolean") throw new Error("Settings value must be a string or boolean");
  if (BOOLEAN_SETTINGS_KEYS.has(key) && typeof value !== "boolean") throw new Error(`${key} must be boolean`);
  if (!BOOLEAN_SETTINGS_KEYS.has(key) && typeof value !== "string") throw new Error(`${key} must be a string`);
}

function createWindow(show = true) {
  if (window) return window;
  window = new BrowserWindow({
    width: 820,
    height: 740,
    minWidth: 680,
    minHeight: 560,
    title: "Codex Voice Settings",
    show,
    frame: false,
    backgroundColor: windowBackground(),
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false
    }
  });
  window.on("maximize", broadcastWindowState);
  window.on("unmaximize", broadcastWindowState);
  window.on("closed", () => { window = null; });
  const devUrl = process.env.VITE_DEV_SERVER_URL;
  if (devUrl) void window.loadURL(devUrl);
  else void window.loadFile(path.join(__dirname, "../dist/index.html"));
  return window;
}

app.setName("codex-voice-settings");
Menu.setApplicationMenu(null);
app.commandLine.appendSwitch("ozone-platform-hint", "auto");
nativeTheme.on("updated", () => window?.setBackgroundColor(windowBackground()));
if (!app.requestSingleInstanceLock()) app.quit();
app.on("second-instance", () => { const current = createWindow(); current.show(); current.focus(); });
app.whenReady().then(async () => {
  ipcMain.handle("codex-voice:load", async () => rememberSettings(await runSettings(["get"])));
  ipcMain.handle("codex-voice:update", async (_event, key: unknown, value: unknown) => {
    validateUpdate(key, value);
    return rememberSettings(await runSettings(["set", key, String(value)]));
  });
  ipcMain.handle("codex-voice:reset", async () => rememberSettings(await runSettings(["reset"])));
  ipcMain.handle("codex-voice:show-preview", showPreview);
  ipcMain.handle("codex-voice:close-preview", closePreview);
  ipcMain.handle("codex-voice:app-info", async () => ({
    ...(await appInfo()),
    appVersion: app.getVersion()
  }) satisfies AppInfo);
  ipcMain.handle("codex-voice:window-state", () => windowState());
  ipcMain.on("codex-voice:minimize", () => window?.minimize());
  ipcMain.on("codex-voice:toggle-maximize", () => {
    if (!window) return;
    if (window.isMaximized()) window.unmaximize();
    else window.maximize();
  });
  ipcMain.on("codex-voice:close", () => window?.close());
  await runSettings(["get"]).then(rememberSettings).catch(() => null);
  createWindow();
  startSettingsMonitor();
  app.on("activate", () => createWindow());
});
app.on("before-quit", event => {
  if (previewProcess && previewProcess.exitCode === null && !quitAfterPreviewCleanup) {
    event.preventDefault();
    if (closingPreviewForQuit) return;
    closingPreviewForQuit = true;
    void closePreview()
      .catch(error => console.error("Could not close preview before quitting", error))
      .finally(() => {
        quitAfterPreviewCleanup = true;
        app.quit();
      });
    return;
  }
  if (monitorReloadTimer) clearTimeout(monitorReloadTimer);
  settingsMonitor?.kill();
});
app.on("window-all-closed", () => app.quit());

async function appInfo(): Promise<Omit<AppInfo, "appVersion">> {
  try {
    const [version, status] = await Promise.all([
      execFileAsync(cli(), ["--version"], { shell: false, windowsHide: true }),
      execFileAsync(cli(), ["--status"], { shell: false, windowsHide: true })
    ]);
    return { version: version.stdout.trim(), cli: cli(), ...parseStatus(status.stdout) };
  } catch {
    return { version: "unavailable", cli: cli(), state: "unknown", extensionActive: false, ubuntu: "unknown", gnomeShell: "unknown" };
  }
}
