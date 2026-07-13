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

export type TranscriptEntry = { id: number; createdAt: number; text: string };
export type TranscriptHistoryPage = { schemaVersion: 1; entries: TranscriptEntry[]; hasMore: boolean };

export interface ElectronAdapter {
  load(): Promise<SettingsDocument>;
  update(key: SettingsKey, value: boolean | string): Promise<SettingsDocument>;
  reset(): Promise<SettingsDocument>;
  loadHistory(offset: number, limit: number, query: string): Promise<TranscriptHistoryPage>;
  copyTranscript(text: string): Promise<void>;
  deleteTranscript(id: number): Promise<void>;
  clearHistory(): Promise<void>;
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

export function parseHistory(text: string): TranscriptHistoryPage {
  const value: unknown = JSON.parse(text);
  if (!object(value) || value.schemaVersion !== 1 || !Array.isArray(value.entries) || typeof value.hasMore !== "boolean" ||
      !value.entries.every(entry => object(entry) && Number.isSafeInteger(entry.id) && Number.isSafeInteger(entry.createdAt) && typeof entry.text === "string")) {
    throw new Error("Invalid Codex Voice transcript history document");
  }
  return value as TranscriptHistoryPage;
}
