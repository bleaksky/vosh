import { useEffect, useState } from 'react';
import {
  PRESETS,
  PRESET_CATEGORIES,
  presetTriggers,
  type PresetCategory,
} from '../lib/presets';
import {
  getUiConfig,
  listTriggers,
  presetsInstall,
  presetsRemove,
  setUiConfig,
} from '../lib/session';

interface Props {
  open: boolean;
  onClose: () => void;
  onError: (message: string) => void;
  /** When true, render only the body so SettingsHub can embed it. */
  chromeless?: boolean;
}

// Categories rendered in this order. Healing & defensive on top so the
// "always-on" presets are right under the user's eye; recall and
// noisy categories below.
const CATEGORY_ORDER: PresetCategory[] = [
  'healing',
  'defensive',
  'buff_falls_self',
  'events',
  'buff_falls_others',
];

export function HighlightsDrawer({ open, onClose, onError, chromeless }: Props) {
  const [enabled, setEnabled] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState('');

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    getUiConfig()
      .then((cfg) => {
        if (cancelled) return;
        setEnabled(new Set(cfg.enabled_presets));
      })
      .catch((e) => {
        if (cancelled) return;
        onError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [open, onError]);

  if (!open && !chromeless) return null;

  const toggle = async (id: string, next: boolean) => {
    const preset = PRESETS.find((p) => p.id === id);
    if (!preset) return;
    setBusy(id);
    try {
      if (next) {
        await presetsInstall(presetTriggers(preset));
        setStatus(`enabled "${preset.name}" (${preset.triggers.length} highlights)`);
      } else {
        const removed = await presetsRemove(id);
        setStatus(`disabled "${preset.name}" (${removed} highlights)`);
      }
      // Persist enabled set to profile.
      const current = await getUiConfig();
      const nextEnabled = new Set(current.enabled_presets);
      if (next) {
        nextEnabled.add(id);
      } else {
        nextEnabled.delete(id);
      }
      const list = Array.from(nextEnabled).sort();
      await setUiConfig({ ...current, enabled_presets: list });
      setEnabled(new Set(list));
    } catch (e) {
      setStatus(`failed: ${String(e)}`);
      onError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const grouped = new Map<PresetCategory, typeof PRESETS>();
  for (const p of PRESETS) {
    const list = grouped.get(p.category) ?? [];
    list.push(p);
    grouped.set(p.category, list);
  }

  const handleReset = async () => {
    console.log('[highlights] reset clicked — starting');
    setStatus('resetting...');
    setBusy('__reset__');
    try {
      let removed = 0;
      for (const p of PRESETS) {
        const n = await presetsRemove(p.id);
        console.log(`[highlights] removed ${n} from preset "${p.id}"`);
        removed += n;
      }
      const defaults = PRESETS.filter((p) => p.defaultEnabled);
      const triggers = defaults.flatMap(presetTriggers);
      console.log(
        `[highlights] installing ${triggers.length} triggers from ${defaults.length} default preset(s)`,
      );
      let installed = 0;
      if (triggers.length > 0) {
        installed = await presetsInstall(triggers);
      }
      const ids = defaults.map((p) => p.id).sort();
      const cfg = await getUiConfig();
      await setUiConfig({ ...cfg, enabled_presets: ids });
      setEnabled(new Set(ids));
      const allTriggers = await listTriggers();
      const presetTriggersInstalled = allTriggers.filter((t) => t.preset);
      console.log(
        '[highlights] reset complete:',
        `removed=${removed}`,
        `installed=${installed}`,
        `enabled=${ids.length} presets`,
        `engine has=${presetTriggersInstalled.length} preset triggers`,
      );
      console.log(
        '[highlights] engine preset triggers:',
        presetTriggersInstalled.map((t) => `${t.preset}: ${t.name}`),
      );
      setStatus(
        `reset OK: removed ${removed}, installed ${installed} highlights across ${ids.length} default presets`,
      );
    } catch (e) {
      console.error('[highlights] reset failed:', e);
      setStatus(`reset FAILED: ${String(e)}`);
      onError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const body = (
    <>
      <p className="drawer-hint">
        Toggle bundled highlight sets without writing regex. Enabled presets
        survive across launches. Your own triggers (from the Triggers drawer
        or .tin import) are unaffected.
      </p>
      <div className="drawer-actions">
        <button
          type="button"
          onClick={() => void handleReset()}
          disabled={busy === '__reset__'}
        >
          {busy === '__reset__' ? 'resetting…' : 'reset to defaults'}
        </button>
        {status && (
          <span className="drawer-status drawer-status-inline">{status}</span>
        )}
      </div>
      {CATEGORY_ORDER.filter((cat) => grouped.has(cat)).map((cat) => (
        <fieldset key={cat} className="drawer-fieldset">
          <legend>{PRESET_CATEGORIES[cat]}</legend>
          <ul className="preset-list">
            {(grouped.get(cat) ?? []).map((preset) => {
              const isOn = enabled.has(preset.id);
              const pending = busy === preset.id;
              return (
                <li key={preset.id} className="preset-row">
                  <label className="preset-toggle">
                    <input
                      type="checkbox"
                      checked={isOn}
                      disabled={pending}
                      onChange={(e) => void toggle(preset.id, e.target.checked)}
                    />
                    <span className="preset-name">{preset.name}</span>
                    <span className="preset-count">
                      {preset.triggers.length} highlight
                      {preset.triggers.length === 1 ? '' : 's'}
                    </span>
                  </label>
                  <p className="preset-desc">{preset.description}</p>
                </li>
              );
            })}
          </ul>
        </fieldset>
      ))}
      <div className="drawer-status">{status}</div>
    </>
  );

  if (chromeless) return body;
  return (
    <aside className="drawer" role="dialog" aria-label="highlight presets">
      <header className="drawer-header">
        <h2>Highlights</h2>
        <button type="button" onClick={onClose} aria-label="close highlights">
          ×
        </button>
      </header>
      {body}
    </aside>
  );
}
