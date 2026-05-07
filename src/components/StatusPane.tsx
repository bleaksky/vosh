import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

interface Vitals {
  hp?: number | string;
  maxhp?: number | string;
  mp?: number | string;
  maxmp?: number | string;
  sp?: number | string;
  maxsp?: number | string;
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

export function StatusPane() {
  const [vitals, setVitals] = useState<Vitals>({});
  const [room, setRoom] = useState<RoomInfo>({});
  const [char, setChar] = useState<CharStatus>({});

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      switch (payload.package) {
        case 'Char.Vitals':
          setVitals(payload.data ?? {});
          break;
        case 'Room.Info':
          setRoom(payload.data ?? {});
          break;
        case 'Char.Name':
        case 'Char.Status':
          setChar((prev) => ({ ...prev, ...(payload.data ?? {}) }));
          break;
        default:
          break;
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals({});
        setRoom({});
        setChar({});
      }
    }).then((fn) => {
      unsubState = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubState?.();
    };
  }, []);

  const charLine = char.fullname || char.name || '-';

  return (
    <section className="status-pane" aria-label="status">
      <header className="pane-header">status</header>
      <div className="status-body">
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
        {char.level !== undefined && (
          <div className="status-row">
            <span className="status-label">level</span>
            <span className="status-value">{char.level}</span>
          </div>
        )}
        <Bar label="hp" current={vitals.hp} max={vitals.maxhp} color="#da3633" />
        <Bar label="mp" current={vitals.mp} max={vitals.maxmp} color="#1f6feb" />
        <Bar label="sp" current={vitals.sp} max={vitals.maxsp} color="#3fb950" />
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
