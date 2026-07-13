export const SETTINGS_KEYS = ["enabled", "show-tray-icon", "keybinding", "language"] as const;
export type SettingsKey = (typeof SETTINGS_KEYS)[number];

export const BOOLEAN_SETTINGS_KEYS: ReadonlySet<SettingsKey> = new Set(["enabled", "show-tray-icon"]);

export type SettingsDocument = {
  schemaVersion: 1;
  enabled: boolean;
  showTrayIcon: boolean;
  keybinding: string;
  language: string;
  overrides: { language: string | null };
};

export type StatusDocument = {
  schemaVersion: 1;
  state: "idle" | "recording" | "transcribing" | "typing";
  extensionActive: boolean;
  ubuntu: string;
  gnomeShell: string;
};

export type AppInfo = {
  version: string;
  appVersion: string;
  cli: string;
  state: StatusDocument["state"] | "unknown";
  extensionActive: boolean;
  ubuntu: string;
  gnomeShell: string;
};

export type WindowState = { maximized: boolean };

export interface ElectronAdapter {
  load(): Promise<SettingsDocument>;
  update(key: SettingsKey, value: boolean | string): Promise<SettingsDocument>;
  reset(): Promise<SettingsDocument>;
  showPreview(): Promise<void>;
  closePreview(): Promise<void>;
  getAppInfo(): Promise<AppInfo>;
  getWindowState(): Promise<WindowState>;
  minimize(): void;
  toggleMaximize(): void;
  close(): void;
  onChanged(callback: (settings: SettingsDocument) => void): () => void;
  onWindowStateChanged(callback: (state: WindowState) => void): () => void;
  onPreviewClosed(callback: () => void): () => void;
}

function object(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseSettings(text: string): SettingsDocument {
  const value: unknown = JSON.parse(text);
  if (!object(value) || value.schemaVersion !== 1 || typeof value.enabled !== "boolean" ||
      typeof value.showTrayIcon !== "boolean" || typeof value.keybinding !== "string" ||
      typeof value.language !== "string" || !object(value.overrides) ||
      !(typeof value.overrides.language === "string" || value.overrides.language === null)) {
    throw new Error("Invalid Codex Voice settings protocol document");
  }
  return value as SettingsDocument;
}

export function parseStatus(text: string): StatusDocument {
  const value: unknown = JSON.parse(text);
  const states: ReadonlyArray<StatusDocument["state"]> = ["idle", "recording", "transcribing", "typing"];
  if (!object(value) || value.schemaVersion !== 1 || !states.includes(value.state as StatusDocument["state"]) ||
      typeof value.extensionActive !== "boolean" || typeof value.ubuntu !== "string" ||
      typeof value.gnomeShell !== "string") {
    throw new Error("Invalid Codex Voice status protocol document");
  }
  return value as StatusDocument;
}
