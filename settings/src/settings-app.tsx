import { useEffect, useMemo, useState, type CSSProperties } from "react";
import type { AppInfo, SettingsDocument, SettingsKey } from "./types";

const languages = [
  ["auto", "Automatic detection"], ["en", "English"], ["es", "Spanish"], ["fr", "French"],
  ["de", "German"], ["it", "Italian"], ["pt", "Portuguese"], ["ja", "Japanese"],
  ["ko", "Korean"], ["zh", "Chinese"]
];

const toKey = (key: SettingsKey): keyof SettingsDocument => ({
  enabled: "enabled", keybinding: "keybinding", "pill-background-color": "pillBackgroundColor",
  "pill-accent-color": "pillAccentColor", language: "language"
}[key]);

export function SettingsApp() {
  const [settings, setSettings] = useState<SettingsDocument | null>(null);
  const [error, setError] = useState("");
  const [capturing, setCapturing] = useState(false);
  const [languageSearch, setLanguageSearch] = useState("");
  const [manualLanguage, setManualLanguage] = useState("");
  const [confirmReset, setConfirmReset] = useState(false);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);

  useEffect(() => { void window.codexVoiceSettings.load().then(setSettings).catch(error => setError(error.message)); }, []);
  useEffect(() => { void window.codexVoiceSettings.getAppInfo().then(setAppInfo).catch(() => undefined); }, []);
  useEffect(() => {
    if (!settings) return;
    setManualLanguage(languages.some(([code]) => code === settings.language) ? "" : settings.language);
  }, [settings?.language]);
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
    const previous = settings;
    setSettings({ ...settings, [toKey(key)]: value });
    setError("");
    try { setSettings(await window.codexVoiceSettings.update(key, value)); }
    catch (caught) { setSettings(previous); setError(caught instanceof Error ? caught.message : "Could not save setting"); }
  }

  const filteredLanguages = useMemo(() => languages.filter(([code, name]) => code === "auto" || name.toLowerCase().includes(languageSearch.toLowerCase())), [languageSearch]);
  if (!settings) return <main className="grid min-h-screen place-items-center text-[color:var(--cv-text)]">{error || "Loading settings…"}</main>;
  const previewStyle = { "--pill-background": settings.pillBackgroundColor, "--pill-accent": settings.pillAccentColor } as CSSProperties;

  return <main className="min-h-screen bg-[color:var(--cv-surface)] p-6 text-[color:var(--cv-text)]">
    <header className="drag-region mx-auto mb-5 max-w-3xl"><h1 className="text-2xl font-semibold">Codex Voice</h1><p className="text-sm text-[color:var(--cv-text-muted)]">Settings apply immediately. Closing this window never stops dictation.</p></header>
    {error && <p role="alert" className="mx-auto mb-4 max-w-3xl rounded-lg border border-[color:var(--cv-danger)] bg-red-950/20 p-3 text-sm">{error}</p>}
    <div className="mx-auto grid max-w-3xl gap-4">
      <section className="settings-card"><h2>General</h2><label className="row"><span><b>Dictation enabled</b><small>Pausing keeps the indicator visible but unregisters its shortcut.</small></span><input aria-label="Dictation enabled" type="checkbox" checked={settings.enabled} onChange={event => void save("enabled", event.target.checked)} /></label><p className="muted">CLI {appInfo?.version ?? "…"} · Ubuntu {appInfo?.ubuntu ?? "…"} · GNOME Shell {appInfo?.gnomeShell ?? "…"} · Extension {appInfo ? appInfo.extensionActive ? "active" : "inactive" : "…"}</p><p className="muted">If the extension is inactive, the CLI uses the GTK3 fallback when XWayland is available. Check that the extension is enabled in Extensions and that <code>ydotoold</code> is running.</p><button className="danger" onClick={() => setConfirmReset(true)}>Reset all settings</button></section>
      <section className="settings-card"><h2>Shortcut</h2><p className="muted">Choose a shortcut with Ctrl, Alt, Super, or a function key. Escape cancels; Delete restores the default.</p><button className={`shortcut ${capturing ? "capturing" : ""}`} onClick={() => setCapturing(true)}>{capturing ? "Press a shortcut…" : settings.keybinding}</button></section>
      <section className="settings-card"><h2>Appearance</h2><div className="grid gap-3 sm:grid-cols-2"><ColorControl label="Pill background" value={settings.pillBackgroundColor} onChange={value => void save("pill-background-color", value)} /><ColorControl label="Accent" value={settings.pillAccentColor} onChange={value => void save("pill-accent-color", value)} /></div><div className="preview-wrap" style={previewStyle}><div className="pill-preview"><span className="pill-icon">●</span><span className="waveform">▁▃▆█▆▃▁</span><button aria-label="Cancel preview">×</button></div><div className="pill-preview"><span>Transcribing…</span><button aria-label="Cancel preview">×</button></div></div></section>
      <section className="settings-card"><h2>Transcription language</h2>{settings.overrides.language && <p className="override">Environment override active: <code>CODEX_VOICE_LANG={settings.overrides.language}</code></p>}<label className="block text-sm">Search common languages<input className="field mt-1" value={languageSearch} onChange={event => setLanguageSearch(event.target.value)} placeholder="Search languages" /></label><select className="field mt-2" value={settings.language} onChange={event => { setManualLanguage(""); void save("language", event.target.value); }}>{filteredLanguages.map(([code, name]) => <option value={code} key={code}>{name}{code === "auto" ? " (default)" : ` (${code})`}</option>)}{!filteredLanguages.some(([code]) => code === settings.language) && <option value={settings.language}>Custom language ({settings.language})</option>}</select><label className="mt-3 block text-sm">Manual language code<input className="field mt-1" value={manualLanguage} onChange={event => setManualLanguage(event.target.value)} placeholder="e.g. en-gb" onBlur={() => { if (manualLanguage.trim()) void save("language", manualLanguage); }} /></label><p className="muted">Automatic detection omits the language hint. Explicit codes are sent as hints. Detection uses an undocumented upstream endpoint, so accuracy and supported languages are not a stable API.</p></section>
    </div>
    {confirmReset && <div className="modal"><div className="settings-card max-w-sm"><h2>Reset all settings?</h2><p className="muted">This restores the shortcut, colours, language, and enabled state.</p><div className="mt-4 flex gap-2"><button onClick={() => setConfirmReset(false)}>Cancel</button><button className="danger" onClick={() => { void window.codexVoiceSettings.reset().then(setSettings).catch(error => setError(error.message)); setConfirmReset(false); }}>Reset</button></div></div></div>}
  </main>;
}

function ColorControl({ label, value, onChange }: { label: string; value: string; onChange(value: string): void }) {
  return <label className="block text-sm">{label}<span className="mt-1 flex items-center gap-2"><input className="native-color" type="color" value={value.slice(0, 7)} onChange={event => onChange(`${event.target.value}${value.slice(7, 9) || "ff"}`)} /><input className="field" value={value} onChange={event => onChange(event.target.value)} /></span></label>;
}
