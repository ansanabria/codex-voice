import { contextBridge, ipcRenderer } from "electron";

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
  getAppInfo: (): Promise<AppInfo> => ipcRenderer.invoke("codex-voice:app-info")
};

contextBridge.exposeInMainWorld("codexVoiceSettings", Object.freeze(api));
