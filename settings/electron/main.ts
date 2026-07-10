import { app, BrowserWindow, ipcMain } from "electron";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import path from "node:path";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const validKeys = new Set(["enabled", "keybinding", "pill-background-color", "pill-accent-color", "language"]);

type SettingsKey = "enabled" | "keybinding" | "pill-background-color" | "pill-accent-color" | "language";
type SettingsDocument = {
  schemaVersion: 1;
  enabled: boolean;
  keybinding: string;
  pillBackgroundColor: string;
  pillAccentColor: string;
  language: string;
  overrides: { language: string | null };
};

let window: BrowserWindow | null = null;

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

function validateUpdate(key: unknown, value: unknown): asserts key is SettingsKey {
  if (typeof key !== "string" || !validKeys.has(key)) throw new Error("Unsupported settings key");
  if (typeof value !== "string" && typeof value !== "boolean") throw new Error("Settings value must be a string or boolean");
  if (key === "enabled" && typeof value !== "boolean") throw new Error("enabled must be boolean");
  if (key !== "enabled" && typeof value !== "string") throw new Error(`${key} must be a string`);
}

function createWindow() {
  if (window) return window;
  window = new BrowserWindow({
    width: 820,
    height: 740,
    minWidth: 680,
    minHeight: 560,
    title: "Codex Voice Settings",
    backgroundColor: "#151a17",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
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
if (!app.requestSingleInstanceLock()) app.quit();
app.on("second-instance", () => { const current = createWindow(); current.show(); current.focus(); });
app.whenReady().then(() => {
  ipcMain.handle("codex-voice:load", () => runSettings(["get"]));
  ipcMain.handle("codex-voice:update", (_event, key: unknown, value: unknown) => {
    validateUpdate(key, value);
    return runSettings(["set", key, String(value)]);
  });
  ipcMain.handle("codex-voice:reset", () => runSettings(["reset"]));
  ipcMain.handle("codex-voice:app-info", async () => ({
    ...(await appInfo()),
    appVersion: app.getVersion()
  }));
  createWindow();
  app.on("activate", () => createWindow());
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
