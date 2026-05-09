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

// All verbs combined into one regex. \b word boundaries keep us from
// matching mid-word; case-sensitivity matters because verb casing
// varies across Aabahran's tiers (lowercase for low-tier hits,
// UPPERCASE for the heavy ones).
const DAMAGE_VERB_RE = new RegExp(
  `\\b(${DAMAGE_TIERS.flatMap((t) => t.verbs).join('|')})\\b`,
  'g',
);

export interface DecorateOpts {
  showLabels: boolean;
  showAverages: boolean;
}

/// Append `[Label ~N]` after each damage verb in the byte stream
/// using dim ANSI so the annotation sits inline without competing
/// for attention. Returns the stream unchanged when no toggles are
/// enabled or no verbs are present, so the cost is one regex test
/// per line.
export function decorateCombat(bytes: Uint8Array, opts: DecorateOpts): Uint8Array {
  if (!opts.showLabels && !opts.showAverages) return bytes;
  const decoder = new TextDecoder('utf-8', { fatal: false });
  const text = decoder.decode(bytes);
  if (!DAMAGE_VERB_RE.test(text)) return bytes;
  // Reset lastIndex since `test` advanced it on a global regex.
  DAMAGE_VERB_RE.lastIndex = 0;
  const decorated = text.replace(DAMAGE_VERB_RE, (match) => {
    const tier = VERB_TO_TIER.get(match);
    if (!tier) return match;
    const parts: string[] = [];
    if (opts.showLabels) parts.push(tier.label);
    if (opts.showAverages) parts.push(`~${tier.avg}`);
    if (parts.length === 0) return match;
    // SGR 2 = faint, SGR 22 = normal intensity. Color of the
    // surrounding text is preserved across the wrap because we don't
    // touch foreground codes.
    return `${match} \x1b[2m[${parts.join(' ')}]\x1b[22m`;
  });
  return new TextEncoder().encode(decorated);
}
