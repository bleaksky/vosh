import { useEffect, useState } from 'react';
import { combatOverlayStore, type CombatOverlaySettings } from '../lib/combatOverlay';

interface Props {
  onError: (message: string) => void;
}

export function CombatPanel({ onError: _onError }: Props) {
  const [settings, setSettings] = useState<CombatOverlaySettings>(
    () => combatOverlayStore.getSettings(),
  );

  useEffect(() => {
    setSettings(combatOverlayStore.getSettings());
    return combatOverlayStore.subscribe(() => {
      setSettings(combatOverlayStore.getSettings());
    });
  }, []);

  const update = (next: Partial<CombatOverlaySettings>) => {
    combatOverlayStore.setSettings(next);
  };

  return (
    <div className="combat-panel">
      <fieldset className="drawer-fieldset">
        <legend>Combat info overlays</legend>
        <p className="drawer-hint">
          Annotate damage verbs in the terminal output with friendly labels
          and rough average damage so you don&apos;t have to memorize the
          table. Reads inline next to the verb in dim brackets, e.g.,{' '}
          <code>MUTILATES [Big Nasty ~40]</code>.
        </p>
        <label>
          <input
            type="checkbox"
            checked={settings.showLabels}
            onChange={(e) => update({ showLabels: e.target.checked })}
          />
          Show damage condition labels (Pretty Hurt, Big Nasty, etc.)
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.showAverages}
            onChange={(e) => update({ showAverages: e.target.checked })}
          />
          Show average damage numbers
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.autoHide}
            onChange={(e) => update({ autoHide: e.target.checked })}
          />
          Auto-hide above level threshold (for veterans)
        </label>
        <label>
          <span>Auto-hide above level</span>
          <input
            type="number"
            min={1}
            max={50}
            value={settings.autoHideAbove}
            disabled={!settings.autoHide}
            onChange={(e) => update({ autoHideAbove: Number(e.target.value) })}
          />
        </label>
      </fieldset>
    </div>
  );
}
