import { useEffect, useState } from 'react';
import type { ConnectionStatus } from './Connect';

interface Props {
  status: ConnectionStatus;
}

function formatClock(date: Date): string {
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

function describeStatus(status: ConnectionStatus): { label: string; tone: string } {
  switch (status.kind) {
    case 'idle':
      return { label: 'idle', tone: 'muted' };
    case 'connecting':
      return { label: `connecting ${status.host}:${status.port}`, tone: 'warn' };
    case 'connected':
      return { label: `${status.host}:${status.port}`, tone: 'ok' };
    case 'error':
      return { label: `error ${status.message}`, tone: 'err' };
  }
}

// tmux-style single-row bottom status line. Left segment carries the
// session/connection state; right segment carries host info and the
// clock. Pipes between segments are accent-colored so the line reads
// like a tmux status bar with the default `#[fg]` separators.
export function StatusBar({ status }: Props) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const tick = () => setNow(new Date());
    tick();
    const id = window.setInterval(tick, 15_000);
    return () => window.clearInterval(id);
  }, []);

  const s = describeStatus(status);

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        <span className="brand-block">[vosh]</span>
        <span className={`statusbar-seg statusbar-tone-${s.tone}`}>{s.label}</span>
      </div>
      <div className="statusbar-right">
        <span className="statusbar-seg statusbar-clock">{formatClock(now)}</span>
      </div>
    </div>
  );
}
