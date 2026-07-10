import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { SettingsApp } from "./settings-app";

const document = {
  schemaVersion: 1 as const,
  enabled: true,
  keybinding: "<Control><Super>space",
  pillBackgroundColor: "#0e1110eb",
  pillAccentColor: "#32d870",
  language: "auto",
  overrides: { language: null }
};

beforeEach(() => {
  window.codexVoiceSettings = {
    load: vi.fn().mockResolvedValue(document),
    update: vi.fn().mockResolvedValue(document),
    reset: vi.fn().mockResolvedValue(document),
    getAppInfo: vi.fn().mockResolvedValue({ version: "0.1.0", appVersion: "0.1.0", cli: "codex-voice", state: "idle", extensionActive: true, ubuntu: "24.04", gnomeShell: "46" })
  };
});

afterEach(cleanup);

test("shows automatic language detection first and saves immediate changes", async () => {
  render(<SettingsApp />);
  expect(await screen.findByRole("option", { name: /automatic detection/i })).toBeInTheDocument();
  fireEvent.click(screen.getByLabelText("Dictation enabled"));
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("enabled", false);
});

test("keeps automatic detection selected while filtering and accepts manual codes", async () => {
  render(<SettingsApp />);
  await screen.findByRole("option", { name: /automatic detection/i });
  fireEvent.change(screen.getByLabelText("Search common languages"), { target: { value: "spanish" } });
  expect(screen.getByRole("combobox")).toHaveValue("auto");
  expect(screen.getByRole("option", { name: /spanish/i })).toBeInTheDocument();

  const manual = screen.getByLabelText("Manual language code");
  fireEvent.change(manual, { target: { value: "EN-GB" } });
  fireEvent.blur(manual);
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("language", "EN-GB");
});
