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

test("shows expanded language options without embedding the recording pill", async () => {
  render(<SettingsApp />);
  expect(await screen.findByLabelText("Language")).toHaveTextContent("Automatic detection");
  expect(screen.queryByLabelText("Monochrome recording pill preview")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Pill theme")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Background color")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("Accent color")).not.toBeInTheDocument();
});

test("offers native-style window controls in the custom title bar", async () => {
  render(<SettingsApp />);
  await screen.findByLabelText("Dictation enabled");
  fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
  fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(window.codexVoiceSettings.minimize).toHaveBeenCalledOnce();
  expect(window.codexVoiceSettings.toggleMaximize).toHaveBeenCalledOnce();
  expect(window.codexVoiceSettings.close).toHaveBeenCalledOnce();
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

test("switches directly to the stable close label while the preview starts", async () => {
  let resolvePreview!: () => void;
  vi.mocked(window.codexVoiceSettings.showPreview).mockReturnValue(new Promise(resolve => { resolvePreview = resolve; }));
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("button", { name: "Show live preview" }));
  expect(screen.getByRole("button", { name: "Close preview" })).toBeDisabled();
  expect(screen.queryByRole("button", { name: "Opening preview…" })).not.toBeInTheDocument();
  resolvePreview();
  expect(await screen.findByRole("button", { name: "Close preview" })).toBeEnabled();
});

test("updates the preview control when the pill is closed from its X button", async () => {
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("button", { name: "Show live preview" }));
  await screen.findByRole("button", { name: "Close preview" });
  (window.codexVoiceSettings as typeof window.codexVoiceSettings & { emitPreviewClosed(): void }).emitPreviewClosed();
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

test("keeps the settings window explicitly opened rather than a login service", async () => {
  render(<SettingsApp />);
  await screen.findByLabelText("Dictation enabled");
  expect(screen.queryByText("Open settings at login")).not.toBeInTheDocument();
  expect(screen.queryByText("Start hidden")).not.toBeInTheDocument();
});
