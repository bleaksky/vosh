import { useEffect, useState } from 'react';
import {
  loadoutsGetState,
  loadoutsSetActive,
  subscribeLoadoutsChanged,
  type LoadoutsState,
  type LoadoutSummary,
} from '../lib/session';

interface Props {
  onError: (message: string | null) => void;
}

// Settings tab for the Path B loadouts. Lists every loadout, lets
// the user toggle which ones are active (multi-select; the runtime
// uses the union of their enabled_groups), and previews the per-
// loadout slot data (description, group chips, auto-match block).
//
// Active toggling is the only mutation in this version. Editing a
// loadout's name, description, enabled_groups, or auto-match lands
// in a follow-up; for now the user authors loadouts by editing
// loadouts.toml directly or by re-running the migration wizard.
export function LoadoutsTab({ onError }: Props) {
  const [state, setState] = useState<LoadoutsState | null>(null);

  const reload = async () => {
    try {
      const s = await loadoutsGetState();
      setState(s);
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  useEffect(() => {
    void reload();
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeLoadoutsChanged(() => {
      if (!cancelled) void reload();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      if (unsub) unsub();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!state) {
    return (
      <div className="loadouts-tab">
        <div className="loadouts-status">loading...</div>
      </div>
    );
  }

  if (!state.path_b_active) {
    return (
      <div className="loadouts-tab">
        <div className="loadouts-empty">
          <div className="loadouts-empty-title">Path B is not active.</div>
          <div className="loadouts-empty-body">
            run the migration from Settings &middot; Profiles to convert your per-profile files into
            a shared catalog + loadouts. once Path B is active, loadouts appear here.
          </div>
        </div>
      </div>
    );
  }

  const handleToggle = async (name: string) => {
    const next = state.active.includes(name)
      ? state.active.filter((n) => n !== name)
      : [...state.active, name];
    try {
      await loadoutsSetActive(next);
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="loadouts-tab">
      <div className="loadouts-help">
        Loadouts are named presets that toggle groups of aliases, triggers, and macros in your
        global catalog. Activate one or more; the runtime enables the union of their groups.
        Activate none to keep the catalog dormant.
      </div>

      <div className="loadouts-active-row">
        <span className="loadouts-active-label">
          {state.active.length === 0 ? 'no loadouts active' : `${state.active.length} active`}
        </span>
        {state.active.length > 0 && (
          <button
            type="button"
            className="settings-btn loadouts-clear-btn"
            onClick={() => void loadoutsSetActive([]).catch((e) => onError(String(e)))}
          >
            deactivate all
          </button>
        )}
      </div>

      <ul className="loadouts-list">
        {state.loadouts.map((l) => (
          <LoadoutRow
            key={l.name}
            loadout={l}
            isActive={state.active.includes(l.name)}
            onToggle={() => void handleToggle(l.name)}
          />
        ))}
      </ul>
    </div>
  );
}

interface RowProps {
  loadout: LoadoutSummary;
  isActive: boolean;
  onToggle: () => void;
}

function LoadoutRow({ loadout, isActive, onToggle }: RowProps) {
  const am = loadout.auto_match;
  return (
    <li className={`loadout-row${isActive ? ' loadout-row-active' : ''}`}>
      <label className="loadout-row-head">
        <input
          type="checkbox"
          checked={isActive}
          onChange={onToggle}
          aria-label={`activate ${loadout.name}`}
        />
        <span className="loadout-row-name">{loadout.name}</span>
        {isActive && <span className="loadout-row-active-tag">[active]</span>}
      </label>
      {loadout.description && <div className="loadout-row-description">{loadout.description}</div>}
      <div className="loadout-row-groups">
        {loadout.enabled_groups.length === 0 ? (
          <span className="loadout-row-groups-empty">(no groups)</span>
        ) : (
          loadout.enabled_groups.map((g) => (
            <span key={g} className="loadout-group-tag">
              {g}
            </span>
          ))
        )}
      </div>
      {am && (am.host || (am.characters && am.characters.length > 0)) && (
        <div className="loadout-row-automatch">
          <span className="loadout-row-automatch-label">auto-match:</span>
          {am.host && (
            <span className="loadout-row-automatch-field">
              host = {am.host}
              {am.port ? `:${am.port}` : ''}
            </span>
          )}
          {am.characters && am.characters.length > 0 && (
            <span className="loadout-row-automatch-field">chars = {am.characters.join(', ')}</span>
          )}
        </div>
      )}
    </li>
  );
}
