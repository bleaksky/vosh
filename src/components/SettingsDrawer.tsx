import { useEffect, useRef, useState } from 'react';
import {
  broadcastTrackedAffects,
  checkForUpdate,
  exportProfile,
  getUiConfig,
  importProfile,
  setUiConfig,
  type ThemeChoice,
} from '../lib/session';
import { applyAndBroadcastTheme } from '../lib/theme';
import { THEMES } from '../lib/themes';
import { dockLayoutStore } from '../lib/docking';

interface Props {
  open: boolean;
  onClose: () => void;
  onError: (message: string) => void;
  /** When true, render only the body (no <aside> wrapper or header)
   *  so the drawer can be embedded inside SettingsHub tabs. */
  chromeless?: boolean;
}

export function SettingsDrawer({ open, onClose, onError, chromeless }: Props) {
  const [toml, setToml] = useState('');
  const [status, setStatus] = useState('');
  const [loading, setLoading] = useState(false);
  const [theme, setTheme] = useState<ThemeChoice>('default');
  const [autoUpdate, setAutoUpdate] = useState(false);
  const [fontFamily, setFontFamily] = useState('');
  const [fontSize, setFontSize] = useState(14);
  const [trackedAffectsText, setTrackedAffectsText] = useState('');
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  // Debounce timer for the tracked-affects textarea auto-save. onBlur
  // alone isn't reliable: closing the settings window with the
  // textarea focused tears the webview down before the blur fires,
  // so the persist invoke never queues. A 500ms idle-save covers
  // that case while staying cheap during typing.
  const trackedAffectsTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    exportProfile()
      .then((text) => {
        if (cancelled) return;
        setToml(text);
        setStatus('loaded current profile');
      })
      .catch((e) => {
        if (cancelled) return;
        setStatus(`load error ${String(e)}`);
        onError(String(e));
      })
      .finally(() => {
        if (cancelled) return;
        setLoading(false);
      });
    getUiConfig()
      .then((cfg) => {
        if (cancelled) return;
        setTheme(cfg.theme);
        setAutoUpdate(cfg.auto_update);
        setFontFamily(cfg.font_family);
        setFontSize(cfg.font_size);
        setTrackedAffectsText((cfg.tracked_affects ?? []).join('\n'));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [open, onError]);

  if (!open && !chromeless) return null;

  const currentTrackedAffects = (): string[] =>
    trackedAffectsText
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);

  const persist = async (overrides: Partial<{
    theme: ThemeChoice;
    auto_update: boolean;
    font_family: string;
    font_size: number;
    tracked_affects: string[];
    enabled_presets: string[];
  }>) => {
    const current = await getUiConfig();
    await setUiConfig({
      theme: overrides.theme ?? theme,
      auto_update: overrides.auto_update ?? autoUpdate,
      font_family: overrides.font_family ?? fontFamily,
      font_size: overrides.font_size ?? fontSize,
      tracked_affects: overrides.tracked_affects ?? currentTrackedAffects(),
      enabled_presets: overrides.enabled_presets ?? current.enabled_presets,
    });
  };

  const handleThemeChange = async (next: ThemeChoice) => {
    setTheme(next);
    void applyAndBroadcastTheme(next);
    try {
      await persist({ theme: next });
      setStatus(`theme set to ${next}`);
    } catch (e) {
      setStatus(`theme save failed ${String(e)}`);
    }
  };

  const handleAutoUpdateChange = async (next: boolean) => {
    setAutoUpdate(next);
    try {
      await persist({ auto_update: next });
      setStatus(next ? 'auto update on' : 'auto update off');
    } catch (e) {
      setStatus(`auto update save failed ${String(e)}`);
    }
  };

  const broadcastFont = (family: string, size: number) => {
    window.dispatchEvent(
      new CustomEvent('vosh:font-changed', { detail: { family, size } }),
    );
  };

  const handleFontFamilyBlur = async () => {
    try {
      await persist({ font_family: fontFamily });
      broadcastFont(fontFamily, fontSize);
      setStatus('font family saved');
    } catch (e) {
      setStatus(`font save failed ${String(e)}`);
    }
  };

  const handleFontSizeChange = async (next: number) => {
    const clamped = Math.max(6, Math.min(64, Math.round(next) || 14));
    setFontSize(clamped);
    try {
      await persist({ font_size: clamped });
      broadcastFont(fontFamily, clamped);
      setStatus(`font size ${clamped}px`);
    } catch (e) {
      setStatus(`font save failed ${String(e)}`);
    }
  };

  const handleTrackedAffectsChange = (next: string) => {
    setTrackedAffectsText(next);
    if (trackedAffectsTimerRef.current !== null) {
      window.clearTimeout(trackedAffectsTimerRef.current);
    }
    trackedAffectsTimerRef.current = window.setTimeout(() => {
      trackedAffectsTimerRef.current = null;
      void handleTrackedAffectsBlur();
    }, 500);
  };

  const handleTrackedAffectsBlur = async () => {
    // Cancel any pending debounce so the explicit blur save isn't
    // immediately followed by a duplicate timer-fired save.
    if (trackedAffectsTimerRef.current !== null) {
      window.clearTimeout(trackedAffectsTimerRef.current);
      trackedAffectsTimerRef.current = null;
    }
    const list = currentTrackedAffects();
    try {
      await persist({ tracked_affects: list });
      // Local CustomEvent for in-window consumers (the chromeless
      // embed inside SettingsHub). Tauri emit for the main window's
      // BottomHUD, which runs in a separate webview and never
      // hears DOM events from settings.
      window.dispatchEvent(
        new CustomEvent('vosh:tracked-affects-changed', { detail: list }),
      );
      await broadcastTrackedAffects(list);
      setStatus(`tracked affects saved (${list.length})`);
    } catch (e) {
      setStatus(`tracked affects save failed ${String(e)}`);
    }
  };

  const handleCheckUpdate = async () => {
    setStatus('checking for updates...');
    try {
      const result = await checkForUpdate();
      if (result.available) {
        setStatus(`update available: ${result.version ?? 'unknown'}`);
      } else {
        setStatus('no updates available');
      }
    } catch (e) {
      setStatus(`update check failed ${String(e)}`);
    }
  };

  const handleResetLayout = () => {
    dockLayoutStore.reset();
    setStatus('dock layout reset');
  };

  const handleApply = async () => {
    try {
      const warnings = await importProfile(toml);
      if (warnings.length === 0) {
        setStatus('profile applied');
      } else {
        setStatus(`applied with ${warnings.length} warning(s): ${warnings.join('; ')}`);
      }
    } catch (e) {
      setStatus(`apply error ${String(e)}`);
      onError(String(e));
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(toml);
      setStatus('copied to clipboard');
    } catch (e) {
      setStatus(`clipboard error ${String(e)}`);
    }
  };

  const handlePaste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      setToml(text);
      setStatus('pasted from clipboard');
    } catch (e) {
      setStatus(`clipboard error ${String(e)}`);
    }
  };

  const handleDownload = () => {
    const blob = new Blob([toml], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'vosh-profile.toml';
    a.click();
    URL.revokeObjectURL(url);
    setStatus('download started');
  };

  const handleUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result ?? '');
      setToml(text);
      setStatus(`loaded file ${file.name}`);
    };
    reader.onerror = () => setStatus('file read failed');
    reader.readAsText(file);
  };

  const body = (
    <>
      <fieldset className="drawer-fieldset">
        <legend>Appearance</legend>
        <div className="theme-picker">
          {THEMES.map((t) => {
            const isActive = theme === t.id || (theme === 'default' && t.id === 'kanso-zen');
            return (
              <button
                key={t.id}
                type="button"
                className={`theme-swatch${isActive ? ' is-active' : ''}`}
                onClick={() => void handleThemeChange(t.id)}
                aria-pressed={isActive}
                title={t.description}
              >
                <div
                  className="theme-swatch-preview"
                  style={
                    {
                      '--preview-bg': t.chrome.surface,
                      '--preview-deep': t.chrome.surfaceDeep,
                      '--preview-accent': t.chrome.accent,
                      '--preview-text': t.chrome.textStrong,
                      '--preview-warn': t.chrome.warn,
                      '--preview-danger': t.chrome.danger,
                    } as React.CSSProperties
                  }
                >
                  <span className="theme-swatch-side" />
                  <span className="theme-swatch-dots">
                    <span style={{ background: t.chrome.danger }} />
                    <span style={{ background: t.chrome.warn }} />
                    <span style={{ background: t.chrome.success }} />
                    <span style={{ background: t.chrome.info }} />
                  </span>
                </div>
                <div className="theme-swatch-meta">
                  <span className="theme-swatch-name">{t.label}</span>
                  <span className="theme-swatch-desc">{t.description}</span>
                </div>
              </button>
            );
          })}
        </div>
        <label>
          <input
            type="checkbox"
            checked={theme === 'system'}
            onChange={(e) =>
              void handleThemeChange(e.target.checked ? 'system' : 'kanso-zen')
            }
          />
          follow system contrast preference
        </label>
        <label>
          <span>font</span>
          <input
            type="text"
            value={fontFamily}
            spellCheck={false}
            onChange={(e) => setFontFamily(e.target.value)}
            onBlur={() => void handleFontFamilyBlur()}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void handleFontFamilyBlur();
              }
            }}
            placeholder="font-family stack"
          />
        </label>
        <label>
          <span>size</span>
          <input
            type="number"
            min={6}
            max={64}
            value={fontSize}
            onChange={(e) => void handleFontSizeChange(Number(e.target.value))}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={autoUpdate}
            onChange={(e) => void handleAutoUpdateChange(e.target.checked)}
          />
          check for updates on launch
        </label>
        <button type="button" onClick={handleCheckUpdate}>
          check for updates now
        </button>
      </fieldset>
      <fieldset className="drawer-fieldset">
        <legend>Layout</legend>
        <p className="drawer-hint">
          Return all docked panels to their default positions in the
          side panel and bottom rail.
        </p>
        <button type="button" onClick={handleResetLayout}>
          reset dock layout
        </button>
      </fieldset>
      <fieldset className="drawer-fieldset">
        <legend>Tracked Affects</legend>
        <p className="drawer-hint">
          One affect name per line. Pills appear in the status bar with the
          remaining duration; missing affects render struck-through with a
          red border.
        </p>
        <textarea
          className="drawer-tracked"
          value={trackedAffectsText}
          spellCheck={false}
          rows={6}
          onChange={(e) => handleTrackedAffectsChange(e.target.value)}
          onBlur={() => void handleTrackedAffectsBlur()}
          placeholder="one affect name per line"
        />
      </fieldset>
      <textarea
        className="drawer-textarea"
        value={toml}
        spellCheck={false}
        onChange={(e) => setToml(e.target.value)}
        disabled={loading}
        rows={20}
      />
      <div className="drawer-actions">
        <button type="button" onClick={handleApply}>
          apply
        </button>
        <button type="button" onClick={handleCopy}>
          copy
        </button>
        <button type="button" onClick={handlePaste}>
          paste
        </button>
        <button type="button" onClick={handleDownload}>
          download
        </button>
        <label className="upload-label">
          upload
          <input
            ref={fileInputRef}
            type="file"
            accept="text/plain,.toml"
            onChange={handleUpload}
            hidden
          />
        </label>
      </div>
      <div className="drawer-status">{status}</div>
    </>
  );

  if (chromeless) return body;
  return (
    <aside className="drawer" role="dialog" aria-label="profile editor">
      <header className="drawer-header">
        <h2>Profile</h2>
        <button type="button" onClick={onClose} aria-label="close settings">
          ×
        </button>
      </header>
      {body}
    </aside>
  );
}
