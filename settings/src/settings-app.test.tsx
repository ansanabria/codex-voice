import { fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { beforeEach, expect, test, vi } from "vitest";
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

test("shows automatic language detection first and saves immediate changes", async () => {
  render(<SettingsApp />);
  expect(await screen.findByRole("option", { name: /automatic detection/i })).toBeInTheDocument();
  fireEvent.click(screen.getByLabelText("Dictation enabled"));
  expect(window.codexVoiceSettings.update).toHaveBeenCalledWith("enabled", false);
});
