import { useEffect, useState } from 'react';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBar } from './components/TopBar';
import { getUiConfig, setUiConfig, type UiConfig } from './lib/session';
import { applyTheme } from './lib/theme';
import { THEMES } from './lib/themes';

const FONT_PRESETS = [
  'BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, monospace',
  'JetBrains Mono, Menlo, monospace',
  'Fira Code, Menlo, monospace',
  'Menlo, Consolas, monospace',
];

// Minimal settings window. Theme + font only. Mirrors the main
// window's frameless Ghostty chrome via the shared TopBar.
export function SettingsApp() {
  const [config, setConfig] = useState<UiConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  // Load current config and reveal the window once painted.
  useEffect(() => {
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      void win.show().then(() => win.setFocus());
    };
    const fallback = window.setTimeout(reveal, 500);
    getUiConfig()
      .then((cfg) => {
        setConfig(cfg);
        applyTheme(cfg.theme);
      })
      .catch((e) => setError(String(e)))
      .finally(reveal);
    return () => window.clearTimeout(fallback);
  }, []);

  const update = (patch: Partial<UiConfig>) => {
    setConfig((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  const save = async () => {
    if (!config) return;
    try {
      await setUiConfig(config);
      applyTheme(config.theme);
      // Broadcast to the main window so the running terminal picks up
      // the new theme + font without a relaunch.
      await emit('vosh://theme-changed', config.theme);
      window.dispatchEvent(
        new CustomEvent('vosh:font-changed', {
          detail: { family: config.font_family, size: config.font_size },
        }),
      );
      setSavedAt(Date.now());
    } catch (e) {
      setError(String(e));
    }
  };

  const close = () => void getCurrentWindow().close();

  return (
    <main className="app settings-app">
      <TopBar brand="[vosh : settings]" showAuxButtons={false} />
      <div className="settings-body">
        {error && <div className="settings-error">error: {error}</div>}
        {config ? (
          <>
            <Row label="theme">
              <select
                value={config.theme}
                onChange={(e) => {
                  update({ theme: e.target.value });
                  applyTheme(e.target.value);
                }}
              >
                {THEMES.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.label}
                  </option>
                ))}
              </select>
            </Row>
            <Row label="font">
              <select
                value={config.font_family}
                onChange={(e) => update({ font_family: e.target.value })}
              >
                {FONT_PRESETS.map((f) => (
                  <option key={f} value={f}>
                    {f.split(',')[0]}
                  </option>
                ))}
              </select>
            </Row>
            <Row label="size">
              <input
                type="number"
                min={9}
                max={32}
                value={config.font_size}
                onChange={(e) =>
                  update({ font_size: Math.max(9, Math.min(32, Number(e.target.value) || 14)) })
                }
              />
              <span className="settings-unit">px</span>
            </Row>
            <div className="settings-actions">
              <button type="button" className="settings-btn" onClick={() => void save()}>
                [save]
              </button>
              <button type="button" className="settings-btn settings-btn-mute" onClick={close}>
                [close]
              </button>
              {savedAt !== null && <span className="settings-saved">saved.</span>}
            </div>
          </>
        ) : (
          !error && <div className="settings-loading">loading…</div>
        )}
      </div>
    </main>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="settings-row">
      <span className="settings-row-label">{label}</span>
      <span className="settings-row-control">{children}</span>
    </label>
  );
}
