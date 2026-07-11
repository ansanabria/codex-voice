import { app, BrowserWindow, ipcMain, Menu, nativeTheme } from "electron";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import { promisify } from "node:util";
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const validKeys = new Set(["enabled", "show-tray-icon", "keybinding", "pill-background-color", "pill-accent-color", "language"]);

type SettingsKey = "enabled" | "show-tray-icon" | "keybinding" | "pill-background-color" | "pill-accent-color" | "language";
type SettingsDocument = {
  schemaVersion: 1;
  enabled: boolean;
  showTrayIcon: boolean;
  keybinding: string;
  pillBackgroundColor: string;
  pillAccentColor: string;
  language: string;
  overrides: { language: string | null };
};

let window: BrowserWindow | null = null;
let settingsMonitor: ChildProcess | null = null;
let monitorReloadTimer: NodeJS.Timeout | null = null;
let monitorReloadRunning = false;
let monitorReloadQueued = false;
let lastSettingsJson = "";

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
  return JSON.parse(stdout) as SettingsDocument;
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
    "/usr/share/glib-2.0/schemas"
  ];
  const environment = { ...process.env };
  const availableSchemaDirectories = schemaDirectories.filter(existsSync);
  if (availableSchemaDirectories.length > 0) {
    environment.GSETTINGS_SCHEMA_DIR = [...availableSchemaDirectories, process.env.GSETTINGS_SCHEMA_DIR].filter(Boolean).join(":");
  }
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

function validateUpdate(key: unknown, value: unknown): asserts key is SettingsKey {
  if (typeof key !== "string" || !validKeys.has(key)) throw new Error("Unsupported settings key");
  if (typeof value !== "string" && typeof value !== "boolean") throw new Error("Settings value must be a string or boolean");
  const booleanKeys = new Set(["enabled", "show-tray-icon"]);
  if (booleanKeys.has(key) && typeof value !== "boolean") throw new Error(`${key} must be boolean`);
  if (!booleanKeys.has(key) && typeof value !== "string") throw new Error(`${key} must be a string`);
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
    backgroundColor: windowBackground(),
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false
    }
  });
  window.on("closed", () => { window = null; });
  const devUrl = process.env.VITE_DEV_SERVER_URL;
  if (devUrl) void window.loadURL(devUrl);
  else void window.loadFile(path.join(__dirname, "../dist/index.html"));
  return window;
}

app.setName("codex-voice-settings");
Menu.setApplicationMenu(null);
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
  ipcMain.handle("codex-voice:app-info", async () => ({
    ...(await appInfo()),
    appVersion: app.getVersion()
  }));
  await runSettings(["get"]).then(rememberSettings).catch(() => null);
  createWindow();
  startSettingsMonitor();
  app.on("activate", () => createWindow());
});
app.on("before-quit", () => {
  if (monitorReloadTimer) clearTimeout(monitorReloadTimer);
  settingsMonitor?.kill();
});
app.on("window-all-closed", () => app.quit());

async function appInfo() {
  try {
    const [version, status] = await Promise.all([
      execFileAsync(cli(), ["--version"], { shell: false, windowsHide: true }),
      execFileAsync(cli(), ["--status"], { shell: false, windowsHide: true })
    ]);
    return { version: version.stdout.trim(), cli: cli(), ...JSON.parse(status.stdout) };
  } catch {
    return { version: "unavailable", cli: cli(), state: "unknown", extensionActive: false, ubuntu: "unknown", gnomeShell: "unknown" };
  }
}
