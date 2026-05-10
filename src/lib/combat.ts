// Combat info overlays — Phase 13.
//
// The damage table is taken straight from the user's TinTin
// `highlights.tin` `CalcDam` function (lines ~10-58). Each verb maps
// to an approximate average damage value plus a friendly label so
// players don't have to memorize the table. The decorator scans
// incoming MUD output for the verbs and injects `[Label ~N]` after
// each one in dim ANSI so it reads inline without competing with the
// rest of the line's coloring.

interface DamageTier {
  verbs: string[];
  avg: number;
  /** Friendly label users can read at a glance. Names lean toward
   *  "Pretty Hurt" / "Big Nasty" idioms the user mentioned plus a
   *  monotonic escalation from Tickle to Cataclysm. */
  label: string;
}

const DAMAGE_TIERS: DamageTier[] = [
  { verbs: ['scratches', 'scratch'], avg: 3, label: 'Tickle' },
  { verbs: ['grazes', 'graze'], avg: 7, label: 'Scrape' },
  { verbs: ['hits', 'hit'], avg: 11, label: 'Glancing' },
  { verbs: ['injures', 'injure'], avg: 15, label: 'Solid' },
  { verbs: ['wounds', 'wound'], avg: 19, label: 'Heavy' },
  { verbs: ['mauls', 'maul'], avg: 23, label: 'Pretty Hurt' },
  { verbs: ['decimates', 'decimate'], avg: 27, label: 'Big Hurt' },
  { verbs: ['devastates', 'devastate'], avg: 31, label: 'Brutal' },
  { verbs: ['maims', 'maim'], avg: 35, label: 'Crippling' },
  { verbs: ['MUTILATES', 'MUTILATE'], avg: 40, label: 'Big Nasty' },
  { verbs: ['LACERATES', 'LACERATE'], avg: 48, label: 'Lethal' },
  { verbs: ['EVISCERATES', 'EVISCERATE'], avg: 55, label: 'Massacre' },
  { verbs: ['DISMEMBERS', 'DISMEMBER'], avg: 59, label: 'Disabling' },
  { verbs: ['MASSACRES', 'MASSACRE'], avg: 73, label: 'Devastating' },
  { verbs: ['MANGLES', 'MANGLE'], avg: 95, label: 'Catastrophic' },
  { verbs: ['DEMOLISHES', 'DEMOLISH'], avg: 116, label: 'Apocalyptic' },
  { verbs: ['OBLITERATES', 'OBLITERATE'], avg: 156, label: 'Annihilation' },
  { verbs: ['DISINTEGRATES', 'DISINTEGRATE'], avg: 213, label: 'Total Wipe' },
  { verbs: ['ANNIHILATES', 'ANNIHILATE'], avg: 288, label: 'Doomstrike' },
  { verbs: ['ERADICATES', 'ERADICATE'], avg: 363, label: 'Cataclysm' },
  { verbs: ['UNSPEAKABLE'], avg: 400, label: 'Unspeakable' },
];

const VERB_TO_TIER = new Map<string, DamageTier>();
for (const tier of DAMAGE_TIERS) {
  for (const v of tier.verbs) VERB_TO_TIER.set(v, tier);
}

// All verbs combined. We only annotate verbs the SERVER wraps in ANSI
// SGR codes — that's how Aabahran marks combat verbs (your TinTin
// `highlights.tin` lines 142-156 match the same shape: `\e[0;33m...\e[0;0m`
// for outgoing damage, `\e[0;31m...\e[0m` for incoming, `\e[0;1;30m...`
// for third-party). Bare occurrences of "hits" / "wound" / "injure" in
// chat lines, room descriptions, or item names don't get the colored
// envelope, so they no longer fire the decorator.
//
// Pattern shape: `<ANSI SGR><verb><ANSI SGR>`. We allow whitespace
// inside the colored span (e.g. `UNSPEAKABLE things`) but require
// the closing SGR within a short window so non-combat lines that just
// happen to color something nearby don't catch a verb.
const VERB_ALTERNATION = DAMAGE_TIERS.flatMap((t) => t.verbs).join('|');
const COMBAT_VERB_RE = new RegExp(
  `(\\x1b\\[[0-9;]+m)([^\\x1b]{0,40}?\\b(?:${VERB_ALTERNATION})\\b[^\\x1b]{0,40}?)(\\x1b\\[[0-9;]+m)`,
  'g',
);
// Inner finder pulls the actual verb out of the colored span so we
// can look up its tier. Same alternation, no anchors.
const VERB_FINDER_RE = new RegExp(`\\b(${VERB_ALTERNATION})\\b`);

export interface DecorateOpts {
  showLabels: boolean;
  showAverages: boolean;
}

/// Append `[Label ~N]` after each damage verb in the byte stream
/// using dim ANSI so the annotation sits inline without competing
/// for attention. Only fires when the verb is wrapped in server-emitted
/// ANSI color (combat context); bare occurrences in chat or descriptions
/// are left untouched. Returns the stream unchanged when no toggles are
/// enabled or no combat-shaped verbs are present.
export function decorateCombat(bytes: Uint8Array, opts: DecorateOpts): Uint8Array {
  if (!opts.showLabels && !opts.showAverages) return bytes;
  const decoder = new TextDecoder('utf-8', { fatal: false });
  const text = decoder.decode(bytes);
  COMBAT_VERB_RE.lastIndex = 0;
  if (!COMBAT_VERB_RE.test(text)) return bytes;
  COMBAT_VERB_RE.lastIndex = 0;
  const decorated = text.replace(
    COMBAT_VERB_RE,
    (full, openSgr: string, span: string, closeSgr: string) => {
      const verbMatch = VERB_FINDER_RE.exec(span);
      if (!verbMatch) return full;
      const tier = VERB_TO_TIER.get(verbMatch[1]);
      if (!tier) return full;
      const parts: string[] = [];
      if (opts.showLabels) parts.push(tier.label);
      if (opts.showAverages) parts.push(`~${tier.avg}`);
      if (parts.length === 0) return full;
      // SGR 2 = faint, SGR 22 = normal intensity. Annotation goes
      // AFTER the closing SGR so it isn't tinted by the combat color.
      return `${openSgr}${span}${closeSgr} \x1b[2m[${parts.join(' ')}]\x1b[22m`;
    },
  );
  return new TextEncoder().encode(decorated);
}
