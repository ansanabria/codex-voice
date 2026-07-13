import type { ElectronAdapter } from "../electron/contract";
export type { AppInfo, SettingsDocument, SettingsKey } from "../electron/contract";

declare global {
  interface Window {
    codexVoiceSettings: ElectronAdapter;
  }
}
