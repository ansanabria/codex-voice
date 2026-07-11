import { contextBridge, ipcRenderer } from "electron";

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
type AppInfo = {
  version: string;
  appVersion: string;
  cli: string;
  state: string;
  extensionActive: boolean;
  ubuntu: string;
  gnomeShell: string;
};

const api = {
  load: (): Promise<SettingsDocument> => ipcRenderer.invoke("codex-voice:load"),
  update: (key: SettingsKey, value: boolean | string): Promise<SettingsDocument> => ipcRenderer.invoke("codex-voice:update", key, value),
  reset: (): Promise<SettingsDocument> => ipcRenderer.invoke("codex-voice:reset"),
  getAppInfo: (): Promise<AppInfo> => ipcRenderer.invoke("codex-voice:app-info"),
  onChanged: (callback: (settings: SettingsDocument) => void): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, settings: SettingsDocument) => callback(settings);
    ipcRenderer.on("codex-voice:settings-changed", listener);
    return () => ipcRenderer.removeListener("codex-voice:settings-changed", listener);
  }
};

contextBridge.exposeInMainWorld("codexVoiceSettings", Object.freeze(api));
