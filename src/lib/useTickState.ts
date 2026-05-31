import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState, onTick, type TickPayload } from './session';

// Subscribe to the backend tick state, the World.Time hour reset, and
// the per-second tickSecs counter. Used by the standalone TickPanel
// and by every inline host (VitalsBar, RoomStrip, AffectsBar,
// StatusBar). Each consumer pays for its own intervals + listeners,
// but the count is at most a handful and the work is cheap.
export function useTickState() {
  const [tick, setTick] = useState<TickPayload | null>(null);
  const [tickSecs, setTickSecs] = useState(0);
  const tickResetAtRef = useRef<number>(Date.now());

  useEffect(() => {
    const id = window.setInterval(() => {
      setTickSecs(Math.floor((Date.now() - tickResetAtRef.current) / 1000));
    }, 250);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;
    let prevHour: number | string | null = null;

    onGmcp((payload) => {
      if (payload.package === 'World.Time' && payload.data && typeof payload.data === 'object') {
        const incoming = payload.data as Record<string, unknown>;
        const hour = incoming.hour as number | string | undefined | null;
        if (hour !== undefined && hour !== null) {
          if (prevHour !== null && prevHour !== hour) {
            tickResetAtRef.current = Date.now();
            setTickSecs(0);
          }
          prevHour = hour;
        }
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onTick((payload) => setTick(payload)).then((fn) => {
      if (cancelled) fn();
      else unsubTick = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setTick(null);
        prevHour = null;
        tickResetAtRef.current = Date.now();
        setTickSecs(0);
      } else if (payload.kind === 'connected') {
        tickResetAtRef.current = Date.now();
        setTickSecs(0);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubGmcp?.();
      unsubTick?.();
      unsubState?.();
    };
  }, []);

  const active = Boolean(tick?.enabled);
  const intervalSec =
    tick?.interval_ms && tick.interval_ms > 0
      ? Math.max(1, Math.round(tick.interval_ms / 1000))
      : 30;
  const warn = active && tickSecs >= Math.max(0, intervalSec - 5);
  return { active, tickSecs, warn };
}
