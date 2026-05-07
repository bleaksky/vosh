import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState, onTick, type TickPayload } from '../lib/session';

type Stat = number | string | undefined;

interface Vitals {
  hp?: Stat;
  maxhp?: Stat;
  // MUDs disagree on names. We accept both common pairs for mana and
  // movement so the bars populate regardless of which the server sends.
  mp?: Stat;
  maxmp?: Stat;
  mana?: Stat;
  maxmana?: Stat;
  sp?: Stat;
  maxsp?: Stat;
  move?: Stat;
  maxmove?: Stat;
  movement?: Stat;
  maxmovement?: Stat;
}

function pickFirst(...candidates: Stat[]): Stat {
  for (const c of candidates) {
    if (c !== undefined && c !== null && c !== '') return c;
  }
  return undefined;
}

interface RoomInfo {
  name?: string;
  area?: string;
  id?: number | string;
}

interface CharStatus {
  name?: string;
  fullname?: string;
  level?: number | string;
  class?: string;
  race?: string;
  alignment?: string | number;
  // Aabahran sometimes nests fields differently across Char.* packages.
  // Allow extra keys so any Char.* push enriches the displayed line.
  [key: string]: unknown;
}

function pct(current: number | string | undefined, max: number | string | undefined): number {
  const c = Number(current);
  const m = Number(max);
  if (!Number.isFinite(c) || !Number.isFinite(m) || m <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((c / m) * 100)));
}

function Bar({
  label,
  current,
  max,
  color,
}: {
  label: string;
  current: number | string | undefined;
  max: number | string | undefined;
  color: string;
}) {
  const value = pct(current, max);
  return (
    <div className="bar-row">
      <div className="bar-label">{label}</div>
      <div className="bar-track">
        <div className="bar-fill" style={{ width: `${value}%`, background: color }} />
        <div className="bar-text">
          {current ?? '-'} / {max ?? '-'}
        </div>
      </div>
    </div>
  );
}

function playBeep() {
  try {
    type WindowWithWebkit = Window & { webkitAudioContext?: typeof AudioContext };
    const w = window as WindowWithWebkit;
    const Ctx = window.AudioContext ?? w.webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.frequency.value = 880;
    osc.type = 'sine';
    osc.connect(gain);
    gain.connect(ctx.destination);
    const now = ctx.currentTime;
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(0.18, now + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.18);
    osc.start(now);
    osc.stop(now + 0.2);
    osc.onended = () => ctx.close();
  } catch {
    // Audio unavailable; ignore.
  }
}

export function StatusPane() {
  const [vitals, setVitals] = useState<Vitals>({});
  const [room, setRoom] = useState<RoomInfo>({});
  const [char, setChar] = useState<CharStatus>({});
  const [tick, setTick] = useState<TickPayload | null>(null);
  const lastFiredRef = useRef<number>(0);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Char.Vitals') {
        setVitals(payload.data ?? {});
        return;
      }
      if (payload.package === 'Room.Info') {
        setRoom(payload.data ?? {});
        return;
      }
      // Any other Char.* push (Name, Status, Info, Stats, Worth, Base...)
      // gets merged into the char snapshot. Different MUDs split the
      // identity fields across different sub-packages, so being permissive
      // means the display fills in regardless of which one Aabahran sends.
      if (payload.package.startsWith('Char.') && payload.data && typeof payload.data === 'object') {
        setChar((prev) => ({ ...prev, ...(payload.data as Record<string, unknown>) }));
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals({});
        setRoom({});
        setChar({});
        setTick(null);
      }
    }).then((fn) => {
      unsubState = fn;
    });

    onTick((payload) => {
      setTick(payload);
      if (payload.fired && payload.sound) {
        // Debounce in case of duplicate fire emits.
        const now = Date.now();
        if (now - lastFiredRef.current > 500) {
          lastFiredRef.current = now;
          playBeep();
        }
      }
    }).then((fn) => {
      unsubTick = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubState?.();
      unsubTick?.();
    };
  }, []);

  const charLine = char.fullname || char.name || '-';
  // Count up from 1 since the last reset, matching the conventional MUD
  // "tick: Ns" display. The remaining_ms field still drives the near-fire
  // flash so the user sees a visual cue right before the next tick.
  const tickElapsed = tick?.enabled
    ? Math.max(1, Math.ceil((tick.interval_ms - tick.remaining_ms) / 1000))
    : null;
  const tickFlash = tick?.enabled && tick.remaining_ms <= 5000;

  return (
    <section className="status-pane" aria-label="status">
      <header className="pane-header">status</header>
      <div className="status-body">
        <div className={`status-row tick-row${tickFlash ? ' tick-flash' : ''}`}>
          <span className="status-label">tick</span>
          <span className="status-value">{tick?.enabled ? `${tickElapsed}s` : 'off'}</span>
        </div>
        <div className="status-row">
          <span className="status-label">char</span>
          <span className="status-value">{charLine}</span>
        </div>
        {char.class && (
          <div className="status-row">
            <span className="status-label">class</span>
            <span className="status-value">{char.class}</span>
          </div>
        )}
        {char.race && (
          <div className="status-row">
            <span className="status-label">race</span>
            <span className="status-value">{char.race}</span>
          </div>
        )}
        {char.level !== undefined && (
          <div className="status-row">
            <span className="status-label">level</span>
            <span className="status-value">{String(char.level)}</span>
          </div>
        )}
        {char.alignment !== undefined && (
          <div className="status-row">
            <span className="status-label">align</span>
            <span className="status-value">{String(char.alignment)}</span>
          </div>
        )}
        <Bar label="hp" current={vitals.hp} max={vitals.maxhp} color="#da3633" />
        <Bar
          label="mp"
          current={pickFirst(vitals.mp, vitals.mana)}
          max={pickFirst(vitals.maxmp, vitals.maxmana)}
          color="#1f6feb"
        />
        <Bar
          label="mv"
          current={pickFirst(vitals.sp, vitals.move, vitals.movement)}
          max={pickFirst(vitals.maxsp, vitals.maxmove, vitals.maxmovement)}
          color="#3fb950"
        />
        <div className="status-row">
          <span className="status-label">room</span>
          <span className="status-value">{room.name ?? '-'}</span>
        </div>
        <div className="status-row">
          <span className="status-label">area</span>
          <span className="status-value">{room.area ?? '-'}</span>
        </div>
      </div>
    </section>
  );
}
