import { useEffect, useRef, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { AppInfo, SettingsDocument, SettingsKey, TranscriptEntry } from "./types";
import appIcon from "../../distribution/codex-voice.png";

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

const settingProperties: Record<SettingsKey, keyof SettingsDocument> = {
  enabled: "enabled", "show-tray-icon": "showTrayIcon", keybinding: "keybinding", language: "language"
};
const toKey = (key: SettingsKey): keyof SettingsDocument => settingProperties[key];

const codeKeys: Readonly<Record<string, string>> = {
  Space: "space", Enter: "Return", NumpadEnter: "KP_Enter", Tab: "Tab", Backquote: "grave",
  Minus: "minus", Equal: "equal", BracketLeft: "bracketleft", BracketRight: "bracketright",
  Backslash: "backslash", Semicolon: "semicolon", Quote: "apostrophe", Comma: "comma",
  Period: "period", Slash: "slash"
};

function acceleratorKey(event: KeyboardEvent): string | null {
  if (["Control", "Alt", "Meta", "Shift"].includes(event.key)) return null;
  if (event.key !== "Unidentified" && event.key !== "Dead") {
    return event.key === " " ? "space" : event.key.length === 1 ? event.key.toLowerCase() : event.key;
  }
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^F(?:[1-9]|[12][0-9]|3[0-5])$/.test(event.code)) return event.code;
  return codeKeys[event.code] ?? null;
}

export function SettingsApp() {
  const [settings, setSettings] = useState<SettingsDocument | null>(null);
  const [error, setError] = useState("");
  const [capturing, setCapturing] = useState(false);
  const [previewState, setPreviewState] = useState<"closed" | "opening" | "open" | "closing">("closed");
  const [confirmReset, setConfirmReset] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [history, setHistory] = useState<TranscriptEntry[]>([]);
  const [historyQuery, setHistoryQuery] = useState("");
  const [historyHasMore, setHistoryHasMore] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(true);
  const [historyExists, setHistoryExists] = useState(false);
  const [confirmClearHistory, setConfirmClearHistory] = useState(false);
  const [clearingHistory, setClearingHistory] = useState(false);
  const [copiedTranscript, setCopiedTranscript] = useState<number | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [maximized, setMaximized] = useState(false);
  const pending = useRef(new Map<SettingsKey, { token: number; value: boolean | string }>());
  const nextSaveToken = useRef(0);
  const nextHistoryRequest = useRef(0);

  function withPending(document: SettingsDocument): SettingsDocument {
    const merged = { ...document };
    for (const [key, write] of pending.current) Object.assign(merged, { [toKey(key)]: write.value });
    return merged;
  }

  useEffect(() => { void window.codexVoiceSettings.load().then(setSettings).catch(error => setError(error.message)); }, []);
  useEffect(() => window.codexVoiceSettings.onChanged(document => setSettings(withPending(document))), []);
  useEffect(() => { void window.codexVoiceSettings.getAppInfo().then(setAppInfo).catch(() => undefined); }, []);
  useEffect(() => { void window.codexVoiceSettings.getWindowState().then(state => setMaximized(state.maximized)).catch(() => undefined); }, []);
  useEffect(() => window.codexVoiceSettings.onWindowStateChanged(state => setMaximized(state.maximized)), []);
  useEffect(() => window.codexVoiceSettings.onPreviewClosed(() => setPreviewState("closed")), []);
  useEffect(() => {
    const timer = window.setTimeout(() => void loadHistory(false, historyQuery), 250);
    return () => window.clearTimeout(timer);
  }, [historyQuery]);
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
      const key = acceleratorKey(event);
      if (!key) {
        if (!["Control", "Alt", "Meta", "Shift"].includes(event.key)) setError("That key could not be identified. Try the shortcut again.");
        return;
      }
      const modifier = event.ctrlKey || event.altKey || event.metaKey;
      const functionKey = /^F(?:[1-9]|[12][0-9]|3[0-5])$/.test(key);
      if (!modifier && !functionKey) { setError("Use Ctrl, Alt, Super, or a function key with a non-modifier key."); return; }
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

  async function showPillPreview() {
    if (previewState !== "closed") return;
    setPreviewState("opening");
    setError("");
    try {
      await window.codexVoiceSettings.showPreview();
      setPreviewState("open");
    } catch (caught) {
      setPreviewState("closed");
      setError(caught instanceof Error ? caught.message : "Could not show pill preview");
    }
  }

  async function closePillPreview() {
    if (previewState !== "open") return;
    setPreviewState("closing");
    setError("");
    try {
      await window.codexVoiceSettings.closePreview();
      setPreviewState("closed");
    } catch (caught) {
      setPreviewState("open");
      setError(caught instanceof Error ? caught.message : "Could not close pill preview");
    }
  }

  async function resetSettings() {
    setResetting(true);
    setError("");
    try {
      setSettings(await window.codexVoiceSettings.reset());
      setConfirmReset(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Could not reset settings");
    } finally {
      setResetting(false);
    }
  }

  async function loadHistory(append: boolean, query = historyQuery) {
    const request = ++nextHistoryRequest.current;
    setHistoryLoading(true);
    try {
      const offset = append ? history.length : 0;
      const page = await window.codexVoiceSettings.loadHistory(offset, 50, query);
      if (request !== nextHistoryRequest.current) return;
      setHistory(current => append ? [...current, ...page.entries] : page.entries);
      setHistoryHasMore(page.hasMore);
      if (!append && query === "") setHistoryExists(page.entries.length > 0);
    } catch (caught) {
      if (request !== nextHistoryRequest.current) return;
      setError(caught instanceof Error ? caught.message : "Could not load transcript history");
    } finally {
      if (request === nextHistoryRequest.current) setHistoryLoading(false);
    }
  }

  async function copyTranscript(entry: TranscriptEntry) {
    try {
      await window.codexVoiceSettings.copyTranscript(entry.text);
      setCopiedTranscript(entry.id);
      window.setTimeout(() => setCopiedTranscript(current => current === entry.id ? null : current), 1500);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "Could not copy transcript"); }
  }

  async function deleteTranscript(id: number) {
    try {
      await window.codexVoiceSettings.deleteTranscript(id);
      setHistory(current => current.filter(entry => entry.id !== id));
      if (historyQuery === "" && history.length === 1 && !historyHasMore) setHistoryExists(false);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "Could not delete transcript"); }
  }

  async function clearHistory() {
    setClearingHistory(true);
    try {
      await window.codexVoiceSettings.clearHistory();
      setHistory([]); setHistoryHasMore(false); setHistoryExists(false); setConfirmClearHistory(false);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "Could not clear transcript history"); }
    finally { setClearingHistory(false); }
  }

  if (!settings) return <main className="grid min-h-screen place-items-center text-[color:var(--cv-text)]">{error || "Loading settings…"}</main>;

  return <main className="app-shell">
    <div className="window-titlebar">
      <span className="window-title">Codex Voice Settings</span>
      <div className="window-controls" aria-label="Window controls">
        <button className="window-control" aria-label="Minimize" onClick={() => window.codexVoiceSettings.minimize()}>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><path d="M6 12h12" /></svg>
        </button>
        <button className="window-control" aria-label={maximized ? "Restore" : "Maximize"} onClick={() => window.codexVoiceSettings.toggleMaximize()}>
          {maximized
            ? <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M9 9h9v9H9z" /><path d="M6 15V6h9" /></svg>
            : <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="1" /></svg>}
        </button>
        <button className="window-control window-close" aria-label="Close" onClick={() => window.codexVoiceSettings.close()}>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12" /></svg>
        </button>
      </div>
    </div>
    <div className="settings-scroll">
      <div className="settings-window">
        <header className="title-bar">
          <img className="app-icon" src={appIcon} alt="" aria-hidden="true" />
          <div className="title-bar-text"><h1>Codex Voice</h1><p>Settings</p></div>
        </header>
        {error && <p role="alert" className="error-alert">{error}</p>}
        <Tabs defaultValue="general" className="settings-tabs" onValueChange={value => { if (value === "transcriptions") void loadHistory(false, historyQuery); }}>
          <TabsList aria-label="Settings sections">
            <TabsTrigger value="general">General</TabsTrigger>
            <TabsTrigger value="transcriptions">Transcriptions</TabsTrigger>
          </TabsList>
          <TabsContent value="general">
            <div className="preference-list">
              <PreferenceSwitch title="Dictation" description="Listen for the global shortcut" label="Dictation enabled" checked={settings.enabled} onChange={checked => void save("enabled", checked)} />
              <PreferenceSwitch title="Show top-bar icon" description="Show Codex Voice controls in the GNOME top bar" label="Show top-bar icon" checked={settings.showTrayIcon} onChange={checked => void save("show-tray-icon", checked)} />
              <section className="preference-row"><div><h2>Keyboard shortcut</h2><p className="muted">Press Escape to cancel</p></div><button className={`shortcut ${capturing ? "capturing" : ""}`} onClick={() => setCapturing(true)}>{capturing ? "Press keys…" : <KeybindingDisplay accelerator={settings.keybinding} />}</button></section>
              <section className="preference-row"><div><h2>Language</h2><p className="muted">Automatic works for most dictation</p></div><SettingsSelect label="Language" value={settings.language} options={languages} onValueChange={value => void save("language", value)} /></section>
              <section className="appearance-row"><div className="appearance-heading"><div><h2>Recording pill</h2><p className="muted">Shown while Codex Voice is listening</p></div><button className="preview-control" onClick={() => previewState === "open" ? void closePillPreview() : void showPillPreview()} disabled={previewState === "opening" || previewState === "closing"}>{previewState === "closed" ? "Show live preview" : "Close preview"}</button></div></section>
            </div>
            <Accordion type="single" collapsible className="advanced"><AccordionItem value="advanced"><AccordionTrigger>Advanced</AccordionTrigger><AccordionContent className="advanced-content">{settings.overrides.language && <p className="override">Language is set by <code>CODEX_VOICE_LANG={settings.overrides.language}</code></p>}<div className="system-info"><p>CLI {appInfo?.version ?? "…"} · Ubuntu {appInfo?.ubuntu ?? "…"} · GNOME {appInfo?.gnomeShell ?? "…"}</p><p>Extension {appInfo ? appInfo.extensionActive ? "active" : "inactive" : "…"}</p></div><div className="reset-setting"><div><h3>Reset settings</h3><p id="reset-description" className="muted">Restore the default shortcut, language, and preferences.</p></div><button className="danger reset-trigger" aria-describedby="reset-description" onClick={() => setConfirmReset(true)}><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M20 11a8.1 8.1 0 0 0-15.5-2M4 5v4h4" /><path d="M4 13a8.1 8.1 0 0 0 15.5 2M20 19v-4h-4" /></svg><span>Reset settings</span></button></div></AccordionContent></AccordionItem></Accordion>
            <p className="save-note" aria-live="polite">Changes are saved automatically</p>
          </TabsContent>
          <TabsContent value="transcriptions">
            <section className="history-section" aria-labelledby="history-title">
              <div className="history-heading"><div><h2 id="history-title">Transcript history</h2><p className="muted">Saved on this device until you delete it</p></div><button className="danger" disabled={!historyExists} onClick={() => setConfirmClearHistory(true)}>Clear history</button></div>
              <input className="field history-search" type="search" aria-label="Search transcript history" placeholder="Search transcripts" value={historyQuery} onChange={event => setHistoryQuery(event.target.value)} />
              <div className="history-list" aria-live="polite">
                {!historyLoading && history.length === 0 && <p className="history-empty">{historyQuery ? "No matching transcripts" : "No transcripts yet"}</p>}
                {history.map(entry => <article className="history-entry" key={entry.id}>
                  <time dateTime={new Date(entry.createdAt).toISOString()}>{new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(entry.createdAt)}</time>
                  <p>{entry.text}</p>
                  <div className="history-actions"><button onClick={() => void copyTranscript(entry)}>{copiedTranscript === entry.id ? "Copied" : "Copy"}</button><button className="danger" aria-label={`Delete transcript from ${new Date(entry.createdAt).toLocaleString()}`} onClick={() => void deleteTranscript(entry.id)}>Delete</button></div>
                </article>)}
              </div>
              {historyHasMore && <button className="load-more" disabled={historyLoading} onClick={() => void loadHistory(true)}>{historyLoading ? "Loading…" : "Load more"}</button>}
            </section>
          </TabsContent>
        </Tabs>
      </div>
    </div>
    {confirmReset && <div className="modal" role="dialog" aria-modal="true" aria-labelledby="reset-title" aria-describedby="reset-copy"><div className="settings-card max-w-sm"><h2 id="reset-title">Reset all settings?</h2><p id="reset-copy" className="muted">This restores startup, tray, shortcut, language, and enabled preferences.</p><div className="mt-4 flex gap-2"><button disabled={resetting} onClick={() => setConfirmReset(false)}>Cancel</button><button className="danger" disabled={resetting} onClick={() => void resetSettings()}>{resetting ? "Resetting…" : "Reset settings"}</button></div></div></div>}
    {confirmClearHistory && <div className="modal" role="dialog" aria-modal="true" aria-labelledby="clear-history-title" aria-describedby="clear-history-copy"><div className="settings-card max-w-sm"><h2 id="clear-history-title">Clear transcript history?</h2><p id="clear-history-copy" className="muted">This permanently deletes every saved transcript. This action cannot be undone.</p><div className="mt-4 flex gap-2"><button disabled={clearingHistory} onClick={() => setConfirmClearHistory(false)}>Cancel</button><button className="danger" disabled={clearingHistory} onClick={() => void clearHistory()}>{clearingHistory ? "Clearing…" : "Clear history"}</button></div></div></div>}
  </main>;
}

function PreferenceSwitch({ title, description, label, checked, onChange }: { title: string; description: string; label: string; checked: boolean; onChange(checked: boolean): void }) {
  return <section className="preference-row"><div><h2>{title}</h2><p className="muted">{description}</p></div><label className="switch"><input aria-label={label} type="checkbox" checked={checked} onChange={event => onChange(event.target.checked)} /><span aria-hidden="true" /></label></section>;
}

function SettingsSelect({ label, value, options, onValueChange }: { label: string; value: string; options: readonly (readonly [string, string])[]; onValueChange(value: string): void }) {
  const hasCurrentValue = options.some(([option]) => option === value);
  return <Select value={value} onValueChange={onValueChange}><SelectTrigger aria-label={label} className="settings-select"><SelectValue /></SelectTrigger><SelectContent position="popper" align="end">{options.map(([option, name]) => <SelectItem value={option} key={option}>{name}</SelectItem>)}{!hasCurrentValue && <SelectItem value={value}>Current value ({value})</SelectItem>}</SelectContent></Select>;
}

function KeybindingDisplay({ accelerator }: { accelerator: string }) {
  const parts = accelerator.match(/<[^>]+>|[^<]+/g) ?? [];
  const modLabels: Record<string, string> = { "<Control>": "Ctrl", "<Alt>": "Alt", "<Super>": "Super", "<Shift>": "Shift" };
  return <>{parts.map((part, i) => {
    const label = modLabels[part] ?? (part === "space" ? "Space" : part);
    return <span key={part + i}>{i > 0 && <span className="kbd-sep" aria-hidden="true">+</span>}<kbd className="kbd">{label}</kbd></span>;
  })}</>;
}
