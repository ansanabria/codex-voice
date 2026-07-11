import { useEffect, useId, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import type { AppInfo, SettingsDocument, SettingsKey } from "./types";

const languages = [
  ["auto", "Automatic detection"], ["en", "English"], ["en-us", "English (United States)"],
  ["en-gb", "English (United Kingdom)"], ["es", "Spanish"], ["es-es", "Spanish (Spain)"],
  ["es-mx", "Spanish (Mexico)"], ["fr", "French"], ["fr-ca", "French (Canada)"],
  ["de", "German"], ["it", "Italian"], ["pt", "Portuguese"], ["pt-br", "Portuguese (Brazil)"],
  ["nl", "Dutch"], ["pl", "Polish"], ["ru", "Russian"], ["uk", "Ukrainian"],
  ["tr", "Turkish"], ["ar", "Arabic"], ["hi", "Hindi"], ["id", "Indonesian"],
  ["ja", "Japanese"], ["ko", "Korean"], ["zh", "Chinese"], ["zh-cn", "Chinese (Simplified)"],
  ["zh-tw", "Chinese (Traditional)"]
] as const;

const pillThemes = {
  dark: { background: "#0f0f0feb", accent: "#10a37f" },
  light: { background: "#fafafaf2", accent: "#10a37f" }
} as const;

const settingProperties: Record<SettingsKey, keyof SettingsDocument> = {
  enabled: "enabled", "show-tray-icon": "showTrayIcon", keybinding: "keybinding", "pill-background-color": "pillBackgroundColor",
  "pill-accent-color": "pillAccentColor", language: "language"
};
const toKey = (key: SettingsKey): keyof SettingsDocument => settingProperties[key];

export function SettingsApp() {
  const [settings, setSettings] = useState<SettingsDocument | null>(null);
  const [error, setError] = useState("");
  const [capturing, setCapturing] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const pending = useRef(new Map<SettingsKey, { token: number; value: boolean | string }>());
  const nextSaveToken = useRef(0);

  function withPending(document: SettingsDocument): SettingsDocument {
    const merged = { ...document };
    for (const [key, write] of pending.current) Object.assign(merged, { [toKey(key)]: write.value });
    return merged;
  }

  useEffect(() => { void window.codexVoiceSettings.load().then(setSettings).catch(error => setError(error.message)); }, []);
  useEffect(() => window.codexVoiceSettings.onChanged(document => setSettings(withPending(document))), []);
  useEffect(() => { void window.codexVoiceSettings.getAppInfo().then(setAppInfo).catch(() => undefined); }, []);
  useEffect(() => {
    if (!confirmReset) return;
    const handler = (event: KeyboardEvent) => { if (event.key === "Escape") setConfirmReset(false); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [confirmReset]);
  useEffect(() => {
    if (!capturing) return;
    const handler = (event: KeyboardEvent) => {
      event.preventDefault();
      if (event.key === "Escape") return setCapturing(false);
      if (event.key === "Backspace" || event.key === "Delete") { setCapturing(false); void save("keybinding", "<Control><Super>space"); return; }
      const modifier = event.ctrlKey || event.altKey || event.metaKey;
      const functionKey = /^F(?:[1-9]|[12][0-9]|3[0-5])$/.test(event.key);
      if (!modifier && !functionKey) { setError("Use Ctrl, Alt, Super, or a function key with a non-modifier key."); return; }
      if (["Control", "Alt", "Meta", "Shift"].includes(event.key)) return;
      const key = event.key === " " ? "space" : event.key.length === 1 ? event.key.toLowerCase() : event.key;
      const accelerator = `${event.ctrlKey ? "<Control>" : ""}${event.altKey ? "<Alt>" : ""}${event.metaKey ? "<Super>" : ""}${event.shiftKey ? "<Shift>" : ""}${key}`;
      setCapturing(false); void save("keybinding", accelerator);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [capturing]);

  async function save(key: SettingsKey, value: boolean | string) {
    if (!settings) return;
    const token = ++nextSaveToken.current;
    pending.current.set(key, { token, value });
    setSettings({ ...settings, [toKey(key)]: value });
    setError("");
    try {
      const saved = await window.codexVoiceSettings.update(key, value);
      if (pending.current.get(key)?.token === token) pending.current.delete(key);
      setSettings(withPending(saved));
    } catch (caught) {
      if (pending.current.get(key)?.token === token) pending.current.delete(key);
      void window.codexVoiceSettings.load().then(document => setSettings(withPending(document))).catch(() => undefined);
      setError(caught instanceof Error ? caught.message : "Could not save setting");
    }
  }

  async function savePillTheme(theme: keyof typeof pillThemes) {
    if (!settings) return;
    const previous = settings;
    const preset = pillThemes[theme];
    setSettings({ ...settings, pillBackgroundColor: preset.background, pillAccentColor: preset.accent });
    setError("");
    try {
      await window.codexVoiceSettings.update("pill-background-color", preset.background);
      setSettings(await window.codexVoiceSettings.update("pill-accent-color", preset.accent));
    } catch (caught) {
      setSettings(previous);
      setError(caught instanceof Error ? caught.message : "Could not save pill theme");
    }
  }

  if (!settings) return <main className="grid min-h-screen place-items-center text-[color:var(--cv-text)]">{error || "Loading settings…"}</main>;
  const previewStyle = { "--pill-background": settings.pillBackgroundColor, "--pill-accent": settings.pillAccentColor } as CSSProperties;
  const pillTheme = settings.pillBackgroundColor.toLowerCase().startsWith("#fa") ? "light" : "dark";

  return <main className="app-shell">
    <div className="settings-window">
      <header className="drag-region app-header"><h1>Codex Voice</h1><p>Settings</p></header>
      {error && <p role="alert" className="error-alert">{error}</p>}
      <div className="preference-list">
        <PreferenceSwitch title="Dictation" description="Listen for the global shortcut" label="Dictation enabled" checked={settings.enabled} onChange={checked => void save("enabled", checked)} />
        <PreferenceSwitch title="Show top-bar icon" description="Show Codex Voice controls in the GNOME top bar" label="Show top-bar icon" checked={settings.showTrayIcon} onChange={checked => void save("show-tray-icon", checked)} />
        <section className="preference-row"><div><h2>Keyboard shortcut</h2><p className="muted">Press Escape to cancel</p></div><button className={`shortcut ${capturing ? "capturing" : ""}`} onClick={() => setCapturing(true)}>{capturing ? "Press keys…" : <KeybindingDisplay accelerator={settings.keybinding} />}</button></section>
        <section className="preference-row"><div><h2>Language</h2><p className="muted">Automatic works for most dictation</p></div><SettingsSelect label="Language" value={settings.language} options={languages} onValueChange={value => void save("language", value)} /></section>
        <section className="appearance-row"><div className="appearance-heading"><div><h2>Recording pill</h2><p className="muted">Shown while Codex Voice is listening</p></div><PillPreview style={previewStyle} /></div><div className="pill-controls"><label className="select-control"><span>Theme</span><SettingsSelect label="Pill theme" value={pillTheme} options={[["dark", "Dark"], ["light", "Light"]] as const} onValueChange={value => void savePillTheme(value as keyof typeof pillThemes)} /></label><ColorControl label="Background" value={settings.pillBackgroundColor} onChange={value => void save("pill-background-color", value)} /><ColorControl label="Accent" value={settings.pillAccentColor} onChange={value => void save("pill-accent-color", value)} /></div></section>
      </div>
      <Accordion type="single" collapsible className="advanced"><AccordionItem value="advanced"><AccordionTrigger>Advanced</AccordionTrigger><AccordionContent className="advanced-content">{settings.overrides.language && <p className="override">Language is set by <code>CODEX_VOICE_LANG={settings.overrides.language}</code></p>}<div className="system-info"><p>CLI {appInfo?.version ?? "…"} · Ubuntu {appInfo?.ubuntu ?? "…"} · GNOME {appInfo?.gnomeShell ?? "…"}</p><p>Extension {appInfo ? appInfo.extensionActive ? "active" : "inactive" : "…"}</p></div><button className="danger" onClick={() => setConfirmReset(true)}>Reset all settings</button></AccordionContent></AccordionItem></Accordion>
      <p className="save-note" aria-live="polite">Changes are saved automatically</p>
    </div>
    {confirmReset && <div className="modal" role="dialog" aria-modal="true" aria-labelledby="reset-title"><div className="settings-card max-w-sm"><h2 id="reset-title">Reset all settings?</h2><p className="muted">This restores startup, tray, shortcut, colour, language, and enabled preferences.</p><div className="mt-4 flex gap-2"><button onClick={() => setConfirmReset(false)}>Cancel</button><button className="danger" onClick={() => { void window.codexVoiceSettings.reset().then(setSettings).catch(error => setError(error.message)); setConfirmReset(false); }}>Reset</button></div></div></div>}
  </main>;
}

function PreferenceSwitch({ title, description, label, checked, onChange }: { title: string; description: string; label: string; checked: boolean; onChange(checked: boolean): void }) {
  return <section className="preference-row"><div><h2>{title}</h2><p className="muted">{description}</p></div><label className="switch"><input aria-label={label} type="checkbox" checked={checked} onChange={event => onChange(event.target.checked)} /><span aria-hidden="true" /></label></section>;
}

function SettingsSelect({ label, value, options, onValueChange }: { label: string; value: string; options: readonly (readonly [string, string])[]; onValueChange(value: string): void }) {
  const hasCurrentValue = options.some(([option]) => option === value);
  return <Select value={value} onValueChange={onValueChange}><SelectTrigger aria-label={label} className="settings-select"><SelectValue /></SelectTrigger><SelectContent position="popper" align="end">{options.map(([option, name]) => <SelectItem value={option} key={option}>{name}</SelectItem>)}{!hasCurrentValue && <SelectItem value={value}>Current value ({value})</SelectItem>}</SelectContent></Select>;
}

function PillPreview({ style }: { style: CSSProperties }) {
  return <div className="pill-preview" style={style}><span className="pill-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 10v1a7 7 0 0 0 14 0v-1"/><line x1="12" y1="18" x2="12" y2="22"/></svg></span><span className="waveform" aria-hidden="true">{[0, 1, 2, 3, 4].map(i => <span key={i} className="wave-bar" />)}</span></div>;
}

function ColorControl({ label, value, onChange }: { label: string; value: string; onChange(value: string): void }) {
  const [draft, setDraft] = useState(value);
  const [validationError, setValidationError] = useState("");
  const errorId = useId();

  useEffect(() => { setDraft(value); setValidationError(""); }, [value]);

  function commitDraft() {
    const candidate = draft.trim();
    if (!/^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/i.test(candidate)) {
      setValidationError("Use #RRGGBB or #RRGGBBAA.");
      return;
    }
    setValidationError("");
    if (candidate !== value) onChange(candidate);
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") event.currentTarget.blur();
    if (event.key === "Escape") {
      setDraft(value);
      setValidationError("");
      event.currentTarget.blur();
    }
  }

  return <div className="color-control"><label htmlFor={`${errorId}-value`}>{label}</label><div><input className="native-color" aria-label={`${label} color`} type="color" value={value.slice(0, 7)} onChange={event => { const next = `${event.target.value}${value.slice(7, 9) || "ff"}`; setDraft(next); setValidationError(""); onChange(next); }} /><input id={`${errorId}-value`} className="field color-value" aria-label={`${label} value`} aria-invalid={Boolean(validationError)} aria-describedby={validationError ? errorId : undefined} value={draft} onChange={event => { setDraft(event.target.value); setValidationError(""); }} onBlur={commitDraft} onKeyDown={handleKeyDown} /></div>{validationError && <span id={errorId} role="alert" className="block mt-1 text-sm text-[color:var(--cv-danger)]">{validationError}</span>}</div>;
}

function KeybindingDisplay({ accelerator }: { accelerator: string }) {
  const parts = accelerator.match(/<[^>]+>|[^<]+/g) ?? [];
  const modLabels: Record<string, string> = { "<Control>": "Ctrl", "<Alt>": "Alt", "<Super>": "Super", "<Shift>": "Shift" };
  return <>{parts.map((part, i) => {
    const label = modLabels[part] ?? (part === "space" ? "Space" : part);
    return <span key={part + i}>{i > 0 && <span className="kbd-sep" aria-hidden="true">+</span>}<kbd className="kbd">{label}</kbd></span>;
  })}</>;
}
