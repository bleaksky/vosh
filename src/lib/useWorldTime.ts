import { useEffect, useState } from 'react';
import { onGmcpPackage, onState } from './session';

// Shape of the Aabahran World.Time GMCP push. Most ROM derivatives
// ship a similar set of fields; we name the ones we actually consume
// and keep the rest as `unknown` so a future addition doesn't require
// a backend round-trip. All fields are optional because not every
// server populates the full set on every push.
export interface WorldTime {
  hour?: number | string;
  day?: number | string;
  month?: number | string;
  year?: number | string;
  daynum?: number | string;
}

// Subscribe to GMCP World.Time pushes. Used by the status bar's MUD
// time chip (and any future panel that wants to show in-game time).
// Mirrors the useTickState shape: one subscription per consumer is
// fine because both the GMCP listener cost and the WorldTime payload
// are tiny.
export function useWorldTime(): WorldTime | null {
  const [time, setTime] = useState<WorldTime | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcpPackage<WorldTime>('World.Time', (data) => {
      if (data && typeof data === 'object') setTime(data);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') setTime(null);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubGmcp?.();
      unsubState?.();
    };
  }, []);

  return time;
}

// Format the World.Time hour field into a short human label. Today
// only the 12-hour AM/PM form is supported; the format choice is
// hard-coded because Aabahran uses 0..23 throughout and the AM/PM
// form is what the tintin nprompt rendered. Returns null when the
// hour is missing or unparseable so callers can hide the chip.
export function formatMudTime(time: WorldTime | null): string | null {
  if (!time) return null;
  const h = parseMudHour(time);
  if (h === null) return null;
  if (h === 0) return '12AM';
  if (h === 12) return '12PM';
  if (h < 12) return `${h}AM`;
  return `${h - 12}PM`;
}

// Color the MUD time chip by time-of-day. Picks from a small palette
// that reads as the sky color at that hour — cool blues at night,
// coral at dawn, gold at midday, orange at dusk, violet in the
// evening. Returns null when the time is unknown so callers can
// fall back to a neutral color.
export function mudTimeColor(time: WorldTime | null): string | null {
  const h = parseMudHour(time);
  if (h === null) return null;
  // 24-hour buckets keyed on the source hour. Boundaries chosen so
  // each band is a visually distinct color step and the eye reads
  // "morning vs afternoon vs dusk" at a glance.
  if (h >= 23 || h < 5) return '#6c7fb8'; // late night — cool blue
  if (h < 7) return '#e89a8a'; //         dawn — coral
  if (h < 11) return '#f0c060'; //        morning — warm yellow
  if (h < 14) return '#ffd44d'; //        midday — bright gold
  if (h < 17) return '#e0a040'; //        afternoon — amber
  if (h < 19) return '#e07050'; //        dusk — orange-red
  if (h < 22) return '#9070b0'; //        evening — violet
  return '#6c7fb8'; //                    late night
}

// Shared hour parser. Aabahran sends `hour` as a number, some other
// ROM derivatives send a string; both accepted. Out-of-range or
// unparseable values produce null.
function parseMudHour(time: WorldTime | null): number | null {
  if (!time) return null;
  const raw = time.hour;
  if (raw === undefined || raw === null || raw === '') return null;
  const h = typeof raw === 'number' ? raw : Number(raw);
  if (!Number.isFinite(h)) return null;
  const n = Math.floor(h);
  if (n < 0 || n > 23) return null;
  return n;
}
