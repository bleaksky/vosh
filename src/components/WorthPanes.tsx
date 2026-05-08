import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

interface Worth {
  gold?: number | string;
  bank?: number | string;
  exp?: number | string;
  tnl?: number | string;
  trains?: number | string;
  practices?: number | string;
  cps?: number | string;
  rps?: number | string;
}

function asValue(value: number | string | undefined): string {
  if (value === undefined || value === '') return '-';
  return String(value);
}

function useWorth(): Worth {
  const [worth, setWorth] = useState<Worth>({});

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Char.Worth' && payload.data && typeof payload.data === 'object') {
        setWorth((prev) => ({ ...prev, ...(payload.data as Worth) }));
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') setWorth({});
    }).then((fn) => {
      unsubState = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubState?.();
    };
  }, []);

  return worth;
}

export function GoldPane() {
  const worth = useWorth();
  return (
    <section className="info-list-pane" aria-label="gold">
      <ul className="info-list">
        <li>
          <span className="info-key">gold</span>
          <span className="info-value">{asValue(worth.gold)}</span>
        </li>
        <li>
          <span className="info-key">bank</span>
          <span className="info-value">{asValue(worth.bank)}</span>
        </li>
        <li>
          <span className="info-key">exp</span>
          <span className="info-value">{asValue(worth.exp)}</span>
        </li>
        <li>
          <span className="info-key">tnl</span>
          <span className="info-value">{asValue(worth.tnl)}</span>
        </li>
        <li>
          <span className="info-key">trains</span>
          <span className="info-value">{asValue(worth.trains)}</span>
        </li>
        <li>
          <span className="info-key">prac</span>
          <span className="info-value">{asValue(worth.practices)}</span>
        </li>
      </ul>
    </section>
  );
}

export function CabalPane() {
  const worth = useWorth();
  return (
    <section className="info-list-pane" aria-label="cabal">
      <ul className="info-list">
        <li>
          <span className="info-key">cabal pts</span>
          <span className="info-value">{asValue(worth.cps)}</span>
        </li>
        <li>
          <span className="info-key">renown pts</span>
          <span className="info-value">{asValue(worth.rps)}</span>
        </li>
      </ul>
    </section>
  );
}
