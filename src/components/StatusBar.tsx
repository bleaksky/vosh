import { useEffect, useState } from 'react';
import {
  getTarget,
  onGmcp,
  onState,
  onTarget,
  type QuickKey,
} from '../lib/session';
import { AffectsBar } from './AffectsBar';

interface CombatState {
  name: string;
  hp?: number;
  condition?: string;
}

interface MoonInfo {
  name?: string;
  active?: boolean;
  phase?: number;
  phase_name?: string;
}

interface MoonsState {
  moons: MoonInfo[];
  eclipse?: boolean;
  triad?: boolean;
  near_alignment?: boolean;
}

function formatClock(date: Date): string {
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

function asNumber(value: unknown): number | undefined {
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (typeof value === 'string') {
    const n = Number(value);
    return Number.isFinite(n) ? n : undefined;
  }
  return undefined;
}

function colorForPct(value: number): string {
  if (value >= 80) return '#87a987';
  if (value >= 60) return '#e6c384';
  if (value >= 40) return '#d99a6c';
  if (value >= 20) return '#e46876';
  return '#7d1d1d';
}

function tintForFill(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const k = 0.25;
  return `rgb(${Math.round(r * k)}, ${Math.round(g * k)}, ${Math.round(b * k)})`;
}

// Aabahran's World.Moons GMCP uses 0=full, 4=new, with 1-3 waning
// and 5-7 waxing. The four geometric circles cover the cardinal
// phases legibly; intermediate phases collapse to the nearest
// neighbour (waxing vs waning is preserved).
function moonGlyphFromIndex(phase: number): string {
  const n = ((Math.round(phase) % 8) + 8) % 8;
  if (n === 0) return '○';                 // full
  if (n >= 1 && n <= 3) return '◑';        // waning (right side dark)
  if (n === 4) return '●';                 // new
  return '◐';                              // waxing (left side dark)
}

function extractCombat(data: unknown): CombatState | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const name = typeof obj.target === 'string' ? obj.target.trim() : '';
  if (!name) return null;
  const out: CombatState = { name };
  const hp = asNumber(obj.hp_pct);
  if (hp !== undefined) out.hp = Math.max(0, Math.min(100, Math.round(hp)));
  if (typeof obj.condition === 'string' && obj.condition.trim()) {
    out.condition = obj.condition.trim();
  }
  return out;
}

export function StatusBar() {
  const [now, setNow] = useState(() => new Date());
  const [combat, setCombat] = useState<CombatState | null>(null);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [quickKeys, setQuickKeys] = useState<QuickKey[]>([]);
  const [moons, setMoons] = useState<MoonsState>({ moons: [] });

  useEffect(() => {
    const wallTick = () => setNow(new Date());
    wallTick();
    const id = window.setInterval(wallTick, 15_000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTarget: (() => void) | undefined;
    let cancelled = false;

    void getTarget()
      .then((snap) => {
        if (cancelled) return;
        setUserTarget(snap.name);
        setQuickKeys(snap.quick_keys);
      })
      .catch(() => undefined);

    onGmcp((payload) => {
      if (payload.package === 'Char.Combat') {
        setCombat(extractCombat(payload.data));
      } else if (
        payload.package === 'World.Moons' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        const data = payload.data as Partial<MoonsState>;
        const next: MoonsState = {
          moons: Array.isArray(data.moons) ? data.moons : [],
        };
        if (data.eclipse !== undefined) next.eclipse = data.eclipse;
        if (data.triad !== undefined) next.triad = data.triad;
        if (data.near_alignment !== undefined) next.near_alignment = data.near_alignment;
        setMoons(next);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onTarget((payload) => {
      setUserTarget(payload.name);
      setQuickKeys(payload.quick_keys);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTarget = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setCombat(null);
        setUserTarget(null);
        setQuickKeys([]);
        setMoons({ moons: [] });
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubGmcp?.();
      unsubState?.();
      unsubTarget?.();
    };
  }, []);

  const configuredKeys = quickKeys.filter((q) => q.verb.length > 0);

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        <AffectsBar />
        {combat && <CombatSeg combat={combat} />}
        {(userTarget || configuredKeys.length > 0) && (
          <span className="statusbar-target">
            {userTarget && (
              <>
                <span className="statusbar-target-label">tar</span>
                <span className="statusbar-target-value">{userTarget}</span>
              </>
            )}
            {configuredKeys.length > 0 && (
              <ul className="statusbar-qkeys" aria-label="quick keys">
                {configuredKeys.map((qk, i) => (
                  <li key={qk.name} className="statusbar-qkey">
                    {i > 0 && (
                      <span className="statusbar-qkey-sep" aria-hidden="true">
                        ·
                      </span>
                    )}
                    <span className="statusbar-qkey-name">{qk.name}</span>
                    <span className="statusbar-qkey-verb">{qk.verb}</span>
                  </li>
                ))}
              </ul>
            )}
          </span>
        )}
      </div>
      <div className="statusbar-right">
        {moons.moons.length > 0 && (
          <span className="statusbar-moons">
            {moons.moons.map((m, i) => (
              <span
                key={m.name ?? i}
                className={`statusbar-moon${m.active ? ' is-active' : ''}`}
                data-tooltip={
                  `${m.name ?? 'moon'}: ${m.phase_name ?? `phase ${m.phase ?? '?'}`}` +
                  (m.active ? ' (active)' : '')
                }
              >
                {moonGlyphFromIndex(m.phase ?? -1)}
              </span>
            ))}
            {moons.eclipse && (
              <span className="statusbar-moon-badge is-eclipse">eclipse</span>
            )}
            {!moons.eclipse && moons.triad && (
              <span className="statusbar-moon-badge is-triad">triad</span>
            )}
            {!moons.eclipse && !moons.triad && moons.near_alignment && (
              <span className="statusbar-moon-badge is-near">near</span>
            )}
          </span>
        )}
        <span className="statusbar-clock">{formatClock(now)}</span>
      </div>
    </div>
  );
}

function CombatSeg({ combat }: { combat: CombatState }) {
  const hp = combat.hp;
  const fill = hp !== undefined ? colorForPct(hp) : '#7aa89f';
  return (
    <div className="statusbar-combat">
      <span className="statusbar-combat-swords" aria-hidden="true">
        ⚔
      </span>
      <span className="statusbar-combat-name">{combat.name}</span>
      {hp !== undefined ? (
        <span className="statusbar-bar-track" style={{ width: 80 }}>
          <span
            className="statusbar-bar-fill"
            style={{ width: `${hp}%`, background: fill }}
            aria-hidden="true"
          />
          <span className="statusbar-bar-text" style={{ color: tintForFill(fill) }}>
            {hp}%
          </span>
        </span>
      ) : (
        <span className="statusbar-combat-unknown">--</span>
      )}
      {combat.condition && (
        <span className="statusbar-combat-condition">{combat.condition}</span>
      )}
    </div>
  );
}
