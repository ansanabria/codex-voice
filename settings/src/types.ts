import type { ElectronAdapter } from "../electron/contract";
export type { AppInfo, SettingsDocument, SettingsKey, TranscriptEntry, TranscriptHistoryPage } from "../electron/contract";

declare global {
  interface Window {
    codexVoiceSettings: ElectronAdapter;
  }
}
