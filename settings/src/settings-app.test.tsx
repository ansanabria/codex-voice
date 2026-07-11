import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { SettingsApp } from "./settings-app";

const document = {
  schemaVersion: 1 as const,
  enabled: true,
  showTrayIcon: true,
  keybinding: "<Control><Super>space",
  pillBackgroundColor: "#0f0f0feb",
  pillAccentColor: "#10a37fff",
  language: "auto",
  overrides: { language: null }
};

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => false);
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
  let changed: ((settings: typeof document) => void) | undefined;
  window.codexVoiceSettings = {
    load: vi.fn().mockResolvedValue(document),
    update: vi.fn().mockResolvedValue(document),
    reset: vi.fn().mockResolvedValue(document),
    getAppInfo: vi.fn().mockResolvedValue({ version: "0.1.0", appVersion: "0.1.0", cli: "codex-voice", state: "idle", extensionActive: true, ubuntu: "24.04", gnomeShell: "46" }),
    onChanged: vi.fn(callback => { changed = callback; return () => { changed = undefined; }; })
  };
  Object.assign(window.codexVoiceSettings, { emitChanged: (settings: typeof document) => changed?.(settings) });
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

test("shows expanded language options and applies pill themes", async () => {
  render(<SettingsApp />);
  expect(await screen.findByLabelText("Language")).toHaveTextContent("Automatic detection");
  expect(screen.getByLabelText("Pill theme")).toHaveTextContent("Dark");
  fireEvent.pointerDown(screen.getByLabelText("Pill theme"), { button: 0, ctrlKey: false, pointerType: "mouse" });
  fireEvent.click(await screen.findByText("Light"));
  await waitFor(() => expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("pill-background-color", "#fafafaf2"));
});

test("keeps the settings window explicitly opened rather than a login service", async () => {
  render(<SettingsApp />);
  await screen.findByLabelText("Dictation enabled");
  expect(screen.queryByText("Open settings at login")).not.toBeInTheDocument();
  expect(screen.queryByText("Start hidden")).not.toBeInTheDocument();
});

test("keeps manual color edits local until a complete hex color is committed", async () => {
  render(<SettingsApp />);
  const input = await screen.findByLabelText("Background value");

  fireEvent.change(input, { target: { value: "#a" } });
  expect(input).toHaveValue("#a");
  expect(window.codexVoiceSettings.update).not.toHaveBeenCalledWith("pill-background-color", "#a");

  fireEvent.change(input, { target: { value: "#aabbccdd" } });
  fireEvent.blur(input);
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("pill-background-color", "#aabbccdd");
});

test("explains invalid manual colors and lets Escape restore the saved value", async () => {
  render(<SettingsApp />);
  const input = await screen.findByLabelText("Accent value");

  fireEvent.change(input, { target: { value: "green" } });
  fireEvent.blur(input);
  expect(await screen.findByText("Use #RRGGBB or #RRGGBBAA.")).toBeVisible();
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(window.codexVoiceSettings.update).not.toHaveBeenCalledWith("pill-accent-color", "green");

  fireEvent.focus(input);
  fireEvent.keyDown(input, { key: "Escape" });
  expect(input).toHaveValue(document.pillAccentColor);
  expect(screen.queryByText("Use #RRGGBB or #RRGGBBAA.")).not.toBeInTheDocument();
});

test("keeps the native color picker immediate and preserves alpha", async () => {
  render(<SettingsApp />);
  const picker = await screen.findByLabelText("Background color");

  fireEvent.change(picker, { target: { value: "#abcdef" } });
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("pill-background-color", "#abcdefeb");
});
