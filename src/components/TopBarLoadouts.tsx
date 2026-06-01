import { useEffect, useRef, useState } from 'react';
import {
  loadoutsGetState,
  loadoutsSetActive,
  subscribeLoadoutsChanged,
  type LoadoutsState,
} from '../lib/session';

// Active-loadouts chrome for the main-window TopBar. Renders nothing
// in legacy mode. In Path B mode shows a single `[loadouts: <count>
// active]` button that opens a dropdown checklist of every loadout.
// Each click toggles a loadout's active state via the existing
// loadouts_set_active backend command. Closes on outside click or
// Escape.
export function TopBarLoadouts() {
  const [state, setState] = useState<LoadoutsState | null>(null);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    const refresh = () =>
      loadoutsGetState()
        .then((s) => {
          if (!cancelled) setState(s);
        })
        .catch(() => {});
    void refresh();
    let unsub: (() => void) | undefined;
    void subscribeLoadoutsChanged(() => {
      if (!cancelled) void refresh();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      if (unsub) unsub();
    };
  }, []);

  // Outside-click + Escape close. Pointerdown so the close fires
  // before any click handler inside an unrelated element.
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!wrapRef.current) return;
      if (e.target instanceof Node && wrapRef.current.contains(e.target)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (!state || !state.path_b_active) return null;

  const activeCount = state.active.length;
  const total = state.loadouts.length;
  const label =
    activeCount === 0
      ? '[loadouts: none]'
      : activeCount === 1
        ? `[loadouts: ${state.active[0]}]`
        : `[loadouts: ${activeCount} active]`;

  const handleToggle = (name: string) => {
    const next = state.active.includes(name)
      ? state.active.filter((n) => n !== name)
      : [...state.active, name];
    void loadoutsSetActive(next).catch((e) => console.error('[topbar-loadouts]', e));
  };

  return (
    <div ref={wrapRef} className="topbar-loadouts">
      <button
        type="button"
        className={`topbar-loadouts-btn${open ? ' topbar-loadouts-btn-open' : ''}`}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((v) => !v)}
      >
        {label}
      </button>
      {open && (
        <div className="topbar-loadouts-menu" role="menu">
          {total === 0 ? (
            <div className="topbar-loadouts-empty">no loadouts defined</div>
          ) : (
            state.loadouts.map((l) => {
              const checked = state.active.includes(l.name);
              return (
                <label
                  key={l.name}
                  className={`topbar-loadouts-item${checked ? ' topbar-loadouts-item-active' : ''}`}
                >
                  <input type="checkbox" checked={checked} onChange={() => handleToggle(l.name)} />
                  <span className="topbar-loadouts-item-name">{l.name}</span>
                  {l.enabled_groups.length > 0 && (
                    <span className="topbar-loadouts-item-groups">
                      {l.enabled_groups.length} groups
                    </span>
                  )}
                </label>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
