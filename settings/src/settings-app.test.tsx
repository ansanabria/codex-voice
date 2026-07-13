import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { SettingsApp } from "./settings-app";

const document = {
  schemaVersion: 1 as const,
  enabled: true,
  showTrayIcon: true,
  keybinding: "<Control><Super>space",
  language: "auto",
  overrides: { language: null }
};

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => false);
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
  let changed: ((settings: typeof document) => void) | undefined;
  let previewClosed: (() => void) | undefined;
  window.codexVoiceSettings = {
    load: vi.fn().mockResolvedValue(document),
    update: vi.fn().mockResolvedValue(document),
    reset: vi.fn().mockResolvedValue(document),
    loadHistory: vi.fn().mockResolvedValue({ schemaVersion: 1, entries: [{ id: 7, createdAt: 1_700_000_000_000, text: "Recovered words" }], hasMore: false }),
    copyTranscript: vi.fn().mockResolvedValue(undefined),
    deleteTranscript: vi.fn().mockResolvedValue(undefined),
    clearHistory: vi.fn().mockResolvedValue(undefined),
    showPreview: vi.fn().mockResolvedValue(undefined),
    closePreview: vi.fn().mockResolvedValue(undefined),
    getAppInfo: vi.fn().mockResolvedValue({ version: "0.1.0", appVersion: "0.1.0", cli: "codex-voice", state: "idle", extensionActive: true, ubuntu: "24.04", gnomeShell: "46" }),
    getWindowState: vi.fn().mockResolvedValue({ maximized: false }),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    onChanged: vi.fn(callback => { changed = callback; return () => { changed = undefined; }; }),
    onWindowStateChanged: vi.fn(() => () => undefined),
    onPreviewClosed: vi.fn(callback => { previewClosed = callback; return () => { previewClosed = undefined; }; })
  };
  Object.assign(window.codexVoiceSettings, { emitChanged: (settings: typeof document) => changed?.(settings), emitPreviewClosed: () => previewClosed?.() });
});

test("reflects settings changed by the tray or CLI while the window is open", async () => {
  render(<SettingsApp />);
  expect(await screen.findByLabelText("Dictation enabled")).toBeChecked();
  const externallyChanged = { ...document, enabled: false, language: "es" };
  (window.codexVoiceSettings as typeof window.codexVoiceSettings & { emitChanged(settings: typeof document): void }).emitChanged(externallyChanged);
  await waitFor(() => expect(screen.getByLabelText("Dictation enabled")).not.toBeChecked());
  expect(screen.getByLabelText("Language")).toHaveTextContent("Spanish");
});

test("does not roll back an optimistic value when a monitor event arrives during its save", async () => {
  let resolveUpdate!: (settings: typeof document) => void;
  vi.mocked(window.codexVoiceSettings.update).mockReturnValue(new Promise(resolve => { resolveUpdate = resolve; }));
  render(<SettingsApp />);
  const toggle = await screen.findByLabelText("Dictation enabled");
  fireEvent.click(toggle);
  expect(toggle).not.toBeChecked();
  (window.codexVoiceSettings as typeof window.codexVoiceSettings & { emitChanged(settings: typeof document): void }).emitChanged(document);
  expect(toggle).not.toBeChecked();
  resolveUpdate({ ...document, enabled: false });
  await waitFor(() => expect(toggle).not.toBeChecked());
});

afterEach(cleanup);

test("shows automatic language detection first and saves immediate changes", async () => {
  render(<SettingsApp />);
  expect(await screen.findByLabelText("Language")).toHaveTextContent("Automatic detection");
  fireEvent.click(screen.getByLabelText("Dictation enabled"));
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("enabled", false);
  fireEvent.click(screen.getByLabelText("Show top-bar icon"));
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("show-tray-icon", false);
});

test("uses the physical key when Electron reports an unidentified shortcut key", async () => {
  vi.mocked(window.codexVoiceSettings.update).mockImplementation(async (key, value) => ({
    ...document,
    keybinding: key === "keybinding" ? String(value) : document.keybinding
  }));
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("button", { name: /Ctrl.*Super.*Space/i }));
  fireEvent.keyDown(window, { key: "Unidentified", code: "Space", ctrlKey: true, metaKey: true });
  await waitFor(() => expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("keybinding", "<Control><Super>space"));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("does not save a shortcut when neither the logical nor physical key is identifiable", async () => {
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("button", { name: /Ctrl.*Super.*Space/i }));
  fireEvent.keyDown(window, { key: "Unidentified", code: "", ctrlKey: true, metaKey: true });
  expect(await screen.findByRole("alert")).toHaveTextContent("That key could not be identified");
  expect(screen.getByRole("button", { name: "Press keys…" })).toBeInTheDocument();
  expect(window.codexVoiceSettings.update).not.toHaveBeenCalledWith("keybinding", expect.anything());
});

test("keeps the live pill preview open until it is explicitly closed", async () => {
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("button", { name: "Show live preview" }));
  expect(window.codexVoiceSettings.showPreview).toHaveBeenCalledOnce();
  expect(await screen.findByRole("button", { name: "Close preview" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Close preview" }));
  expect(window.codexVoiceSettings.closePreview).toHaveBeenCalledOnce();
  expect(await screen.findByRole("button", { name: "Show live preview" })).toBeEnabled();
});

test("frames resetting settings as a confirmed destructive action", async () => {
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("button", { name: "Advanced" }));
  fireEvent.click(await screen.findByRole("button", { name: "Reset settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Reset all settings?" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Reset settings" }));
  await waitFor(() => expect(window.codexVoiceSettings.reset).toHaveBeenCalledOnce());
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
});

test("searches, copies, and deletes transcript history", async () => {
  render(<SettingsApp />);
  fireEvent.mouseDown(await screen.findByRole("tab", { name: "Transcriptions" }), { button: 0, ctrlKey: false });
  expect(await screen.findByText("Recovered words")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Search transcript history"), { target: { value: "words" } });
  await waitFor(() => expect(window.codexVoiceSettings.loadHistory).toHaveBeenCalledWith(0, 50, "words"));
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));
  expect(window.codexVoiceSettings.copyTranscript).toHaveBeenCalledWith("Recovered words");
  fireEvent.click(screen.getByRole("button", { name: /Delete transcript from/ }));
  await waitFor(() => expect(window.codexVoiceSettings.deleteTranscript).toHaveBeenCalledWith(7));
  expect(screen.queryByText("Recovered words")).not.toBeInTheDocument();
});

test("refreshes transcript history when its tab is opened", async () => {
  vi.mocked(window.codexVoiceSettings.loadHistory)
    .mockResolvedValueOnce({ schemaVersion: 1, entries: [], hasMore: false })
    .mockResolvedValueOnce({ schemaVersion: 1, entries: [{ id: 8, createdAt: 1_700_000_000_001, text: "Newly dictated words" }], hasMore: false });
  render(<SettingsApp />);
  fireEvent.mouseDown(await screen.findByRole("tab", { name: "Transcriptions" }), { button: 0, ctrlKey: false });
  expect(await screen.findByText("Newly dictated words")).toBeInTheDocument();
  expect(window.codexVoiceSettings.loadHistory).toHaveBeenCalledTimes(2);
});

test("requires confirmation before clearing all transcript history", async () => {
  render(<SettingsApp />);
  fireEvent.mouseDown(await screen.findByRole("tab", { name: "Transcriptions" }), { button: 0, ctrlKey: false });
  await screen.findByText("Recovered words");
  fireEvent.click(screen.getByRole("button", { name: "Clear history" }));
  const dialog = screen.getByRole("dialog", { name: "Clear transcript history?" });
  expect(within(dialog).getByText(/cannot be undone/i)).toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "Clear history" }));
  await waitFor(() => expect(window.codexVoiceSettings.clearHistory).toHaveBeenCalledOnce());
});

test("ignores transcript results from a stale search request", async () => {
  let resolveOld!: (page: { schemaVersion: 1; entries: { id: number; createdAt: number; text: string }[]; hasMore: boolean }) => void;
  let resolveNew!: (page: { schemaVersion: 1; entries: { id: number; createdAt: number; text: string }[]; hasMore: boolean }) => void;
  vi.mocked(window.codexVoiceSettings.loadHistory).mockImplementation((_offset, _limit, query) => {
    if (query === "old") return new Promise(resolve => { resolveOld = resolve; });
    if (query === "new") return new Promise(resolve => { resolveNew = resolve; });
    return Promise.resolve({ schemaVersion: 1, entries: [], hasMore: false });
  });
  render(<SettingsApp />);
  fireEvent.mouseDown(await screen.findByRole("tab", { name: "Transcriptions" }), { button: 0, ctrlKey: false });
  const search = await screen.findByLabelText("Search transcript history");
  fireEvent.change(search, { target: { value: "old" } });
  await waitFor(() => expect(window.codexVoiceSettings.loadHistory).toHaveBeenCalledWith(0, 50, "old"));
  fireEvent.change(search, { target: { value: "new" } });
  await waitFor(() => expect(window.codexVoiceSettings.loadHistory).toHaveBeenCalledWith(0, 50, "new"));
  resolveNew({ schemaVersion: 1, entries: [{ id: 2, createdAt: 2, text: "Newest query result" }], hasMore: false });
  expect(await screen.findByText("Newest query result")).toBeInTheDocument();
  resolveOld({ schemaVersion: 1, entries: [{ id: 1, createdAt: 1, text: "Stale query result" }], hasMore: true });
  await waitFor(() => expect(screen.queryByText("Stale query result")).not.toBeInTheDocument());
  expect(screen.getByText("Newest query result")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
});
