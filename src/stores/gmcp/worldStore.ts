import { asNumber, asText } from '../store';
import { createGmcpStore } from './gmcpStore';

// Game time and the moons for the status line. Aabahran sends
// World.Time `{hour, day, month, year, sunlight, sky}` at login and on
// each weather tick, and World.Moons only at login and when a phase
// changes, so a per mount copy (useWorldTime, the StatusBar moons) went
// blank for hours after a remount. Aabahran sends no minute. The field
// fills in for servers that do.

export interface WorldTime {
  /** 0..23. */
  hour: number | null;
  /** 0..59, from servers that send `minute` or `min`. */
  minute: number | null;
  day: number | null;
  month: number | null;
  year: number | null;
  /** Aabahran sends dark, rise, light or set. */
  sunlight: string | null;
  /** Aabahran sends cloudless, cloudy, raining or lightning. */
  sky: string | null;
}

export interface Moon {
  name: string;
  active: boolean;
  /** 0 new, 1..3 waxing, 4 full, 5..7 waning. */
  phase: number | null;
  /** The server's wording, like "half-lit and growing". */
  phase_name: string | null;
}

export interface Moons {
  moons: Moon[];
  eclipse: boolean;
  triad: boolean;
  near_alignment: boolean;
}

export interface WorldState {
  time: WorldTime | null;
  moons: Moons | null;
}

function inRange(value: unknown, min: number, max: number): number | null {
  const n = asNumber(value);
  if (n === null) return null;
  const whole = Math.floor(n);
  return whole >= min && whole <= max ? whole : null;
}

/** Parse World.Time. null when the payload is not an object. */
export function parseWorldTime(data: unknown): WorldTime | null {
  if (!data || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  return {
    hour: inRange(d.hour, 0, 23),
    minute: inRange(d.minute ?? d.min, 0, 59),
    day: asNumber(d.day),
    month: asNumber(d.month),
    year: asNumber(d.year),
    sunlight: asText(d.sunlight),
    sky: asText(d.sky),
  };
}

/** Parse World.Moons. null when the payload is not an object. */
export function parseMoons(data: unknown): Moons | null {
  if (!data || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  const moons: Moon[] = [];
  if (Array.isArray(d.moons)) {
    for (const raw of d.moons) {
      if (!raw || typeof raw !== 'object') continue;
      const m = raw as Record<string, unknown>;
      const name = asText(m.name);
      if (!name) continue;
      moons.push({
        name,
        active: m.active === true,
        phase: inRange(m.phase, 0, 7),
        phase_name: asText(m.phase_name),
      });
    }
  }
  return {
    moons,
    eclipse: d.eclipse === true,
    triad: d.triad === true,
    near_alignment: d.near_alignment === true,
  };
}

export type MoonPhaseWord = 'new' | 'waxing' | 'full' | 'waning';

/** One word for a phase index, per the Aabahran moon table. */
export function moonPhaseWord(phase: number | null): MoonPhaseWord | null {
  if (phase === null) return null;
  if (phase === 0) return 'new';
  if (phase === 4) return 'full';
  if (phase >= 1 && phase <= 3) return 'waxing';
  if (phase >= 5 && phase <= 7) return 'waning';
  return null;
}

/** The words for a moon's icon, like "Lysenties, half-lit and
 *  growing". The server's phase name, else the phase word, else the
 *  name alone. */
export function moonTitle(moon: Moon): string {
  const phase = moon.phase_name ?? moonPhaseWord(moon.phase);
  return phase ? `${moon.name}, ${phase}` : moon.name;
}

export type MoonAlignment = 'Eclipse' | 'Triad' | 'Near alignment';

/** The one word the sky earns beside the moons. An eclipse over the
 *  triad over a near alignment. Null when the sky is quiet. */
export function moonAlignment(moons: Moons | null): MoonAlignment | null {
  if (!moons) return null;
  if (moons.eclipse) return 'Eclipse';
  if (moons.triad) return 'Triad';
  if (moons.near_alignment) return 'Near alignment';
  return null;
}

/** Short moon label, like "Lysenties waxing". Uses the first moon in
 *  the sky, else the first moon listed. */
export function moonLabel(moons: Moons | null): string | null {
  if (!moons || moons.moons.length === 0) return null;
  const moon = moons.moons.find((m) => m.active) ?? moons.moons[0];
  const word = moonPhaseWord(moon.phase);
  return word ? `${moon.name} ${word}` : moon.name;
}

const store = createGmcpStore<WorldState>({
  state: { time: null, moons: null },
  packages: {
    'World.Time': (world, data) => {
      const time = parseWorldTime(data);
      return time ? { ...world, time } : world;
    },
    'World.Moons': (world, data) => {
      const moons = parseMoons(data);
      return moons ? { ...world, moons } : world;
    },
  },
  // The status line keeps the last game time after the link drops,
  // next to Not connected. The moons go, and a new connection starts
  // from nothing since it may reach another world.
  connection: (world, payload) => {
    if (payload.kind === 'disconnected') return { ...world, moons: null };
    if (payload.kind === 'connecting') return { time: null, moons: null };
    return world;
  },
});

export const startWorldStore = store.start;
export const getWorld = store.get;
export const useWorld = store.use;
