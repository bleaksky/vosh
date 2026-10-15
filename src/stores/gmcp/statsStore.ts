import { statsSnapshotGet } from '../../ipc/stats';
import { createSessionStore } from '../sessionStore';
import { asNumber } from '../store';

// The numbers on your score sheet, from Char.Stats. Aabahran sends it at
// login and on a prompt after one of them moved (gmcp.c,
// gmcp_send_stats), so a window that opens later asks the backend for
// the last one. A disconnect puts the pane back to waiting.

/** The five stats, in the score sheet's order. */
export const STAT_KEYS = ['str', 'int', 'wis', 'dex', 'con'] as const;
export type StatKey = (typeof STAT_KEYS)[number];

/** The four kinds of armor class, in the score sheet's order. */
export const ARMOR_KEYS = ['slash', 'pierce', 'blunt', 'magic'] as const;
export type ArmorKey = (typeof ARMOR_KEYS)[number];

/** The five saves, in the score sheet's order. */
export const SAVE_KEYS = ['spell', 'afflictive', 'maledictive', 'mental', 'breath'] as const;
export type SaveKey = (typeof SAVE_KEYS)[number];

/** One stat. `cur` is the stat in use, 3 to `max`. `perm` is what you
 *  trained, capped at `max`. `over` is how far gear and spells push it
 *  past `max`, which counts for nothing. */
export interface StatValue {
  cur: number;
  perm: number;
  max: number;
  over: number;
}

/** One kind of armor class and the share of it that counts. Lower is
 *  better. */
export interface ArmorValue {
  ac: number;
  pct: number | null;
}

/** What Char.Stats carries. A part the packet leaves out reads null. */
export interface Stats {
  stats: Readonly<Record<StatKey, StatValue | null>>;
  /** How many of the luck bar's steps are lit. The game never sends
   *  the value itself. */
  luck: { bar: number; of: number } | null;
  hitroll: number | null;
  /** The share of hitroll that counts, the percent beside it on the
   *  score sheet. */
  hitrollPct: number | null;
  damroll: number | null;
  armor: Readonly<Record<ArmorKey, ArmorValue | null>>;
  /** Lower is better. Above zero works against you. */
  saves: Readonly<Record<SaveKey, number | null>>;
}

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function statOf(value: unknown): StatValue | null {
  const o = record(value);
  if (!o) return null;
  const cur = asNumber(o.cur);
  const max = asNumber(o.max);
  if (cur === null || max === null) return null;
  return { cur, perm: asNumber(o.perm) ?? cur, max, over: Math.max(0, asNumber(o.over) ?? 0) };
}

function luckOf(value: unknown): Stats['luck'] {
  const o = record(value);
  const bar = asNumber(o?.bar);
  const of = asNumber(o?.of);
  if (bar === null || of === null || of <= 0) return null;
  return { bar: Math.min(Math.max(0, Math.round(bar)), of), of: Math.round(of) };
}

function keyed<K extends string, V>(
  keys: readonly K[],
  read: (key: K) => V | null,
): Readonly<Record<K, V | null>> {
  return Object.fromEntries(keys.map((key) => [key, read(key)])) as Record<K, V | null>;
}

/** Read a Char.Stats packet, or null when it carries nothing. */
export function parseStats(data: unknown): Stats | null {
  const d = record(data);
  if (!d) return null;
  const ac = record(d.ac);
  const acPct = record(d.ac_pct);
  const saves = record(d.saves);
  return {
    stats: keyed(STAT_KEYS, (key) => statOf(d[key])),
    luck: luckOf(d.luck),
    hitroll: asNumber(d.hitroll),
    hitrollPct: asNumber(d.hitroll_pct),
    damroll: asNumber(d.damroll),
    armor: keyed(ARMOR_KEYS, (key) => {
      const value = asNumber(ac?.[key]);
      return value === null ? null : { ac: value, pct: asNumber(acPct?.[key]) };
    }),
    saves: keyed(SAVE_KEYS, (key) => asNumber(saves?.[key])),
  };
}

const store = createSessionStore<Stats | null>({
  state: null,
  packages: { 'Char.Stats': (state, data) => parseStats(data) ?? state },
  snapshot: {
    ask: statsSnapshotGet,
    take: (state, data) => parseStats(data) ?? state,
  },
});

export const startStatsStore = store.start;
export const getStats = store.get;
export const useStats = store.use;
