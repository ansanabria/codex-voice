export type SettingsKey = "enabled" | "keybinding" | "pill-background-color" | "pill-accent-color" | "language";

export type SettingsDocument = {
  schemaVersion: 1;
  enabled: boolean;
  keybinding: string;
  pillBackgroundColor: string;
  pillAccentColor: string;
  language: string;
  overrides: { language: string | null };
};

export type AppInfo = {
  version: string;
  appVersion: string;
  cli: string;
  state: string;
  extensionActive: boolean;
  ubuntu: string;
  gnomeShell: string;
};

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
