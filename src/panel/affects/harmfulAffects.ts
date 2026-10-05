import { normalizeAffectName } from '../../lib/affects';

// Affects the Affects pane lists first, in the danger color, among the
// ones you do not track. Char.Affects carries no harmful flag (the
// Aabahran emitter sends kind, name, duration, level, location and
// modifier only), so the client keys on the affect name. Names are
// what the server sends, lower case, and match through
// normalizeAffectName.
//
// The stock ROM set comes first so other ROM derivatives get a useful
// default. The Aabahran additions come from its skill table: offensive
// spells and maledictions whose wear off message reads as a debuff
// lifting from you. Cooldown markers that share the table (insect
// swarm, planar seal, field of fear and the like) stay out because
// the caster carries those, not the victim.
export const HARMFUL_AFFECTS: readonly string[] = [
  // Stock ROM.
  'blindness',
  'calm',
  'change sex',
  'charm person',
  'chill touch',
  'curse',
  'dirt kicking',
  'energy drain',
  'faerie fire',
  'faerie fog',
  'fear',
  'plague',
  'poison',
  'sleep',
  'slow',
  'weaken',
  // Aabahran spells and skills.
  'age',
  'apathy',
  'batter',
  'bio manipulation',
  'blackjack',
  'blasphemy',
  'brain death',
  'brittleness',
  'corruption',
  'damnation',
  'death grasp',
  'demonfire',
  'demonfrost',
  'demonic visage',
  'deteriorate',
  'disintegrate',
  'drained',
  'dysentery',
  'ecstasy',
  'ego whip',
  'enfeeblement',
  'entangle',
  'entropic touch',
  'feeblemind',
  'feedback',
  'fire breath',
  'flashfire',
  'forest mist',
  'forget',
  'ghoul touch',
  'glob acid',
  'hex',
  'hold undead',
  'hurricane',
  'hypnosis',
  'infernal link',
  'insomnia',
  'lifedrain',
  'lingering shatter',
  'lure',
  'magic missile',
  'majestic solo',
  'mana leak',
  'mass hysteria',
  'mental knife',
  'mind blast',
  'mind disruption',
  'mind thrust',
  'molecular leash',
  'nerve amp',
  'numbness',
  'paralyze',
  'petrify',
  'phantom grasp',
  'poison dust',
  'psychic purge',
  'ripple',
  'rust',
  'shatter',
  'shoulder smash',
  'shrink',
  'silence',
  'smoke bomb',
  'soul pump',
  'soul tear',
  'spell vise',
  'spook',
  'strangle',
  'tame',
  'tele lock',
  'thunderclap',
  'turn undead',
  'unminding',
  'veil of darkness',
  'webbing',
  // Aabahran thief traps and sabotage.
  'broken straps',
  'false teeth',
  'flash oil',
  'shredded straps',
  'slick shoes',
  'slippy grips',
  'trip wire',
  // Aabahran alchemy poisons.
  'astral slow',
  'blackspore',
  'bone stigma',
  'flesheater oil',
  'living death',
  'lotus toxin',
  'nettle essence',
  'numbing gel',
  'scouring serum',
  'slugwort drought',
  'stonefoot powder',
];

/** Normalized lookup set for a harmful list. The Affects view builds
 *  one per call, so callers can pass a per profile list later. */
export function harmfulSet(names: Iterable<string>): Set<string> {
  const out = new Set<string>();
  for (const name of names) {
    const key = normalizeAffectName(name);
    if (key.length > 0) out.add(key);
  }
  return out;
}
