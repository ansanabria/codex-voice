import { contextBridge, ipcRenderer } from "electron";
import type { AppInfo, ElectronAdapter, SettingsDocument, SettingsKey, WindowState } from "./contract.js";

const api = {
  load: (): Promise<SettingsDocument> => ipcRenderer.invoke("codex-voice:load"),
  update: (key: SettingsKey, value: boolean | string): Promise<SettingsDocument> => ipcRenderer.invoke("codex-voice:update", key, value),
  reset: (): Promise<SettingsDocument> => ipcRenderer.invoke("codex-voice:reset"),
  showPreview: (): Promise<void> => ipcRenderer.invoke("codex-voice:show-preview"),
  closePreview: (): Promise<void> => ipcRenderer.invoke("codex-voice:close-preview"),
  getAppInfo: (): Promise<AppInfo> => ipcRenderer.invoke("codex-voice:app-info"),
  getWindowState: (): Promise<WindowState> => ipcRenderer.invoke("codex-voice:window-state"),
  minimize: (): void => ipcRenderer.send("codex-voice:minimize"),
  toggleMaximize: (): void => ipcRenderer.send("codex-voice:toggle-maximize"),
  close: (): void => ipcRenderer.send("codex-voice:close"),
  onChanged: (callback: (settings: SettingsDocument) => void): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, settings: SettingsDocument) => callback(settings);
    ipcRenderer.on("codex-voice:settings-changed", listener);
    return () => ipcRenderer.removeListener("codex-voice:settings-changed", listener);
  },
  onWindowStateChanged: (callback: (state: WindowState) => void): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, state: WindowState) => callback(state);
    ipcRenderer.on("codex-voice:window-state", listener);
    return () => ipcRenderer.removeListener("codex-voice:window-state", listener);
  },
  onPreviewClosed: (callback: () => void): (() => void) => {
    const listener = () => callback();
    ipcRenderer.on("codex-voice:preview-closed", listener);
    return () => ipcRenderer.removeListener("codex-voice:preview-closed", listener);
  }
} satisfies ElectronAdapter;

contextBridge.exposeInMainWorld("codexVoiceSettings", Object.freeze(api));
