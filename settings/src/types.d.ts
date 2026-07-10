import type { AppInfo, SettingsDocument, SettingsKey } from "../electron/preload";

declare global {
  interface Window {
    codexVoiceSettings: {
      load(): Promise<SettingsDocument>;
      update(key: SettingsKey, value: boolean | string): Promise<SettingsDocument>;
      reset(): Promise<SettingsDocument>;
      getAppInfo(): Promise<AppInfo>;
    };
  }
}

export {};
