// Highlight preset library — Phase 12.
//
// Each preset is a named bundle of triggers a user can toggle from the
// Highlights drawer. Toggling on installs every trigger in the bundle
// (tagged with the preset id so we can find them again); toggling off
// removes everything tagged with that id. User-authored triggers are
// untouched either way.
//
// Patterns are POSIX-flavored regex compatible with Rust's `regex`
// crate. Captures use $1, $2 in Replace templates. Highlight actions
// take a HighlightStyle and wrap matched text with ANSI on the way to
// the terminal.
//
// Seeded from the user's `~/tintin/highlights.tin`. Categories are
// chosen so noise-heavy event groups (others' buff churn, others'
// recall) can be toggled independently from must-see ones (your own
// buffs falling, your own recall).

import type { HighlightStyle, TriggerRecord } from './session';
import { colorize } from './colorTokens';

export type PresetCategory =
  | 'healing'
  | 'defensive'
  | 'disarm_buff'
  | 'events'
  | 'loot'
  | 'labels';

export interface Preset {
  id: string;
  category: PresetCategory;
  name: string;
  description: string;
  defaultEnabled: boolean;
  triggers: Omit<TriggerRecord, 'preset'>[];
}

export const PRESET_CATEGORIES: Record<PresetCategory, string> = {
  healing: 'Healing & Cure',
  defensive: 'Defensive Combat',
  disarm_buff: 'Disarm & Buff Fade',
  events: 'Combat & Spell Events',
  loot: 'Loot & Progression',
  labels: 'Potion & Herb Labels',
};

// Helper to build a highlight trigger compactly. Default priority of 5
// matches the user's TinTin highlight priority so user-authored
// triggers at the same number stay in stable order.
function highlight(
  name: string,
  pattern: string,
  style: HighlightStyle,
  priority = 5,
): Omit<TriggerRecord, 'preset'> {
  return {
    name,
    pattern,
    priority,
    enabled: true,
    action: { kind: 'highlight', style },
  };
}

// Replace trigger that injects ANSI directly into the substitution so
// the resulting line carries its own coloring without a separate
// highlight pass. Used for the Magick-style ///NAME RECALLED///
// banners.
function replace(
  name: string,
  pattern: string,
  template: string,
  priority = 5,
): Omit<TriggerRecord, 'preset'> {
  return {
    name,
    pattern,
    priority,
    enabled: true,
    action: { kind: 'replace', template: colorize(template) },
  };
}

// Color shorthands. The trigger backend's HighlightStyle.fg only
// accepts the 16 ANSI named colors (black, red, green, ...,
// bright_white). The terminal theme maps those to the Kanso palette,
// so using named colors here means presets pick up whatever theme is
// active.
const GREEN: HighlightStyle = { fg: 'bright_green' };
const RED: HighlightStyle = { fg: 'bright_red', bold: true };

// Damage verb alternation, lifted from the user's TinTin `CalcDam`
// table. Pairs cover the conjugated form ("punch decimates") and
// the bare form ("decimate") since some attack messages skip the s.
const DAMAGE_VERBS = [
  'scratches',
  'scratch',
  'grazes',
  'graze',
  'hits',
  'hit',
  'injures',
  'injure',
  'wounds',
  'wound',
  'mauls',
  'maul',
  'decimates',
  'decimate',
  'devastates',
  'devastate',
  'maims',
  'maim',
  'MUTILATES',
  'MUTILATE',
  'LACERATES',
  'LACERATE',
  'EVISCERATES',
  'EVISCERATE',
  'DISMEMBERS',
  'DISMEMBER',
  'MASSACRES',
  'MASSACRE',
  'MANGLES',
  'MANGLE',
  'DEMOLISHES',
  'DEMOLISH',
  'OBLITERATES',
  'OBLITERATE',
  'DISINTEGRATES',
  'DISINTEGRATE',
  'ANNIHILATES',
  'ANNIHILATE',
  'ERADICATES',
  'ERADICATE',
  'UNSPEAKABLE',
];
const DAMAGE_VERB_ALT = DAMAGE_VERBS.join('|');

// Top-tier ROM hits wrap the verb in one of:
//   *** verb ***
//   === verb ===
//   >>> verb <<<
//   <<< verb >>>
//   does verb things        Aabahran's top-tier spell phrasing
//   do verb things          ...same form when you cause it (You do
//                           UNSPEAKABLE things to him!)
// Both wrapper slots accept any of *, =, >, < so all four bracket
// combinations match (previous version had asymmetric classes and
// failed entirely on `<<< VERB >>>`).
const DAMAGE_VERB_WRAPPED = `(?:(?:[*=><]{3}|does|do) )?(?:${DAMAGE_VERB_ALT})(?: (?:[*=><]{3}|things))?`;

// Token table now lives in src/lib/colorTokens.ts so the trigger form
// editor can use the same grammar (and the inverse).

export const PRESETS: Preset[] = [
  // ── Healing & Cure ────────────────────────────────────────────────
  {
    id: 'healing_basics',
    category: 'healing',
    name: 'Cure & heal messages',
    description:
      'You feel a lot better! / You are no longer poisoned. / etc. ' +
      'Greens up the line so you spot heals at a glance.',
    defaultEnabled: true,
    triggers: [
      highlight('cure.feel_lot_better', 'You feel a lot better!$', GREEN),
      highlight('cure.feel_better', 'You feel better\\.$', GREEN),
      highlight('cure.feel_much_better', 'You feel much better!$', GREEN),
      highlight('cure.righteous', 'You feel righteous\\.$', GREEN),
      highlight('cure.less_sick', 'You feel less sick\\.$', GREEN),
      highlight('cure.no_longer_poisoned', 'You are no longer poisoned\\.$', GREEN),
      highlight('cure.less_tired', 'You feel less tired\\.$', GREEN),
    ],
  },

  // ── Defensive Combat ──────────────────────────────────────────────
  // Tintin-style. Routine defenses fade to dark grey (matches the
  // user's `highlights.tin` <g08> so combat reads at a glance.
  // Patterns anchor to a trailing period so dramatic outcomes ending
  // in "!" (e.g., "You dodge X's attack and redirect the momentum!")
  // stay full-bright.
  {
    id: 'defensive_combat',
    category: 'defensive',
    name: 'Parries, dodges, blocks',
    description:
      'Greys out routine defensive saves (parry, dodge, block, etc.) ' +
      'using the same dark-grey shade your tintin uses (<g08>).',
    defaultEnabled: true,
    triggers: [
      // Generic "You dodge X." / "You parry X." — matches the bare
      // form in highlights.tin line 97. Lower priority so the more
      // specific "block / dual parry / reverse" replacements below
      // can win on lines they uniquely identify.
      replace(
        'def.dodge_or_parry',
        '^You (?:dodge|parry) .+\\.$',
        '{fg:240}$0{reset}',
      ),
      // Redirect-momentum counter (ends in `!` so it's not caught by
      // the generic period-anchored pattern above). Same dim treatment
      // as a normal dodge.
      replace(
        'def.redirect_momentum',
        '^You .+ and redirect the momentum!$',
        '{fg:240}$0{reset}',
      ),
      // Shadow-blend evade — assassin/thief flavor defense, ends in
      // `!` like the redirect.
      replace(
        'def.shadows_evade',
        '^You blend into the shadows, evading .+!$',
        '{fg:240}$0{reset}',
      ),
      // Parry with hand specified (highlights.tin line 93).
      replace(
        'def.parry_hand',
        '^You parry .+ attack with your (?:first|second) hand\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace(
        'def.block_shield',
        '^You block .+ with your shield\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace(
        'def.block_weapon',
        '^You block .+ attack with your weapon\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      // Block and attempt to strike (highlights.tin line 89).
      replace(
        'def.block_attempt',
        '^You block .+ attack and attempt to strike at the brief opening\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace(
        'def.dual_parry',
        '^You dual parry .+ attack\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace(
        'def.reverse',
        '^You reverse .+ attack.*\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      // Stagger out of attack (highlights.tin line 95).
      replace(
        'def.stagger',
        '^You stagger wildly out of .+ attack\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace(
        'def.swing_through',
        '^You swing right through .+ blurred image\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace(
        'def.misses',
        '^.+ swings wildly and misses you by a mile\\.$',
        '{fg:240}$0{reset}',
      ),
      replace(
        'def.shadows_envelop',
        '^Shadows envelop .+\\.$',
        '{fg:253}$0{reset}',
      ),
      replace(
        'def.terra_shield',
        '^Your Terra shield deflects the attack\\.$',
        '{fg:240}$0{reset}',
      ),
      // Faith save (highlights.tin line 99).
      replace(
        'def.faith',
        '^Your faith holding fast, you stop the blow with .+ power\\.$',
        '{fg:240}$0{reset}',
      ),
      // Giant blade deflect (highlights.tin line 88).
      replace(
        'def.giant_blade',
        '^The giant blade deflects .+ attack\\.$',
        '{fg:240}$0{reset}',
      ),
    ],
  },

  // ── COMBAT — DISARM / BUFF FADE SUBSTITUTES ───────────────────────
  // tintin lines 105–106 (disarms) + 130–134 (buff fades). Both
  // groups share the same `<018>## <178>...<088>` styling in the
  // user's TinTin so they live in one preset here. Disarms reword
  // PRIMARY/SECONDARY explicitly.
  {
    id: 'disarm_buff_fade',
    category: 'disarm_buff',
    name: 'Disarm & buff fade',
    description: 'Disarms and buff drops from highlights.tin 105-134.',
    defaultEnabled: true,
    triggers: [
      replace(
        'disarm.secondary',
        '^(.+) disarms you and sends your secondary weapon flying!$',
        '{bold_red}##{reset} {fg:178}$1 disarms you and sends your SECONDARY weapon flying!{reset}',
      ),
      replace(
        'disarm.primary',
        '^(.+) disarms you and sends your weapon flying!$',
        '{bold_red}##{reset} {fg:178}$1 disarms you and sends your PRIMARY weapon flying!{reset}',
      ),
      replace(
        'buff.protective_shield',
        '^(.+) protective shield dissipates\\.$',
        '{bold_red}##{reset} {fg:178}$1 protective shield dissipates.{reset}',
      ),
      replace(
        'buff.protective_aura',
        '^The protective aura around your body fades\\.$',
        '{bold_red}##{reset} {fg:178}The protective aura around your body fades.{reset}',
      ),
      replace(
        'buff.stoneskin',
        '^The shards of metal protecting you fall to the ground\\.$',
        '{bold_red}##{reset} {fg:178}The shards of metal protecting you fall to the ground.{reset}',
      ),
      replace(
        'buff.sanctuary',
        '^The white aura around (.+) fades\\.$',
        '{bold_red}##{reset} {fg:178}The white aura around $1 fades.{reset}',
      ),
      replace(
        'buff.spell_turning',
        '^Your shield of spell turning collapses\\.$',
        '{bold_red}##{reset} {fg:178}Your shield of spell turning collapses.{reset}',
      ),
    ],
  },

  // ── Terror weapon drop ────────────────────────────────────────────
  // tintin line 124 is an #ACTION (auto-rearm) and the line itself
  // is not auto-highlighted by tintin, but the user wants it
  // visually flagged anyway. Bold bright red.
  {
    id: 'terror_events',
    category: 'events',
    name: 'Terror weapon drop',
    description: 'Bold red on the terror-induced weapon drop line.',
    defaultEnabled: true,
    triggers: [
      highlight(
        'terror.drop',
        '^Filled with terror, your weapon slips through your slippery fingers\\.$',
        RED,
      ),
    ],
  },

  // ── Outgoing damage ───────────────────────────────────────────────
  // Every "Your <attack> <VERB> <target><.|!>" line gets the verb
  // recolored to a burnt-amber tone. Matches both lowercase verbs
  // (hits, scratches) and uppercase ones (LACERATES, DISINTEGRATES).
  {
    id: 'combat_outgoing',
    category: 'events',
    name: 'Your damage verbs (amber)',
    description:
      "Highlights damage verbs in lines that start with 'Your ...' " +
      'so outgoing hits stand out without recoloring the rest of the line.',
    defaultEnabled: true,
    triggers: [
      // Mirrors the TinTin `You%1` form so both "Your kick LACERATES
      // X" and "You LACERATE X" / "You miss X" lines fire — the
      // optional `(?:r .+?)?` lets group 1 capture "You " or "Your
      // <attack> ".
      replace(
        'combat.outgoing',
        `^(You(?:r .+?)? )(${DAMAGE_VERB_WRAPPED})( .+[!.])$`,
        '{fg:253}$1{reset}{fg:214}$2{reset}{fg:253}$3{reset}',
        7,
      ),
      // Outgoing miss — `<aee>` pale cyan on the verb, `<g21>` body
      // (matching the damage-hit body color).
      replace(
        'combat.outgoing_miss',
        '^(You(?:r .+?)? )(misses|miss)( .+[!.])$',
        '{fg:253}$1{reset}{fg:152}$2{reset}{fg:253}$3{reset}',
        7,
      ),
    ],
  },

  // ── Incoming damage ───────────────────────────────────────────────
  // Lines like "<Enemy>'s <attack> <VERB> you[!.]" get the whole line
  // toned to grey 244 (TinTin's <g12>) with the verb in pink-red 217
  // (TinTin's <fbb>). Misses get pale cyan 152 (TinTin's <aee>).
  {
    id: 'combat_incoming',
    category: 'events',
    name: 'Damage to you (grey line, red verb)',
    description:
      'Tones lines where something hits you to grey, with the damage ' +
      'verb in red and missed swings in pale cyan.',
    defaultEnabled: true,
    triggers: [
      replace(
        'combat.incoming',
        `^(.+? )(${DAMAGE_VERB_WRAPPED})( you[!.])$`,
        '{fg:244}$1{reset}{fg:210}$2{reset}{fg:244}$3{reset}',
        7,
      ),
      // Incoming miss — `<aee>` pale cyan on the verb, `<g12>` body.
      replace(
        'combat.incoming_miss',
        '^(.+? )(misses|miss)( you[!.])$',
        '{fg:244}$1{reset}{fg:152}$2{reset}{fg:244}$3{reset}',
        7,
      ),
    ],
  },

  // ── LOOT & PROGRESSION ────────────────────────────────────────────
  // tintin lines 170-174. Numbers in near-white, body in mid grey.
  {
    id: 'loot_progression',
    category: 'loot',
    name: 'Gold / xp / level / skill-up',
    description: 'Loot and progression lines from highlights.tin 170-174.',
    defaultEnabled: true,
    triggers: [
      replace(
        'loot.gold',
        '^You get (\\d+) gold coins from (.+)\\.$',
        '{fg:249}You get {fg:230}$1 {fg:249}gold coins from $2.{reset}',
      ),
      replace(
        'loot.skill_up',
        '^You have become better at (.+)!$',
        '{fg:120}You have become better at {fg:230}$1{fg:120}!{reset}',
      ),
      replace(
        'loot.level',
        '^You raise a level!!  You gain:  (\\d+)/\\d+ hit points, (\\d+)/\\d+ mana, (\\d+)/\\d+ move, and (\\d+) practices\\.$',
        '{fg:120}You raise a level!! You gain {fg:230}$1 hp{fg:120}, {fg:230}$2 mn{fg:120}, {fg:230}$3 mv{fg:120} and {fg:230}$4 practices.{reset}',
      ),
      replace(
        'loot.xp',
        '^You receive (\\d+) experience points\\.$',
        '{fg:248}You receive {fg:230}$1 {fg:248}experience points.{reset}',
      ),
    ],
  },

  // ── POTION LABELS ────────────────────────────────────────────────
  // tintin lines 180-186. Appends a grey `(spell)` parenthetical
  // to each potion description.
  {
    id: 'potion_labels',
    category: 'labels',
    name: 'Potion labels',
    description: 'Spell-name annotations for potions from highlights.tin 180-186.',
    defaultEnabled: true,
    triggers: [
      replace('potion.brown', 'a bubbly brown potion', 'a bubbly brown potion {fg:248}(cure serious){reset}'),
      replace('potion.clear', 'a bubbly clear potion', 'a bubbly clear potion {fg:248}(invisibility){reset}'),
      replace('potion.crimson', 'a bubbly crimson potion', 'a bubbly crimson potion {fg:248}(fireball){reset}'),
      replace('potion.green', 'a bubbly green potion', 'a bubbly green potion {fg:248}(haste){reset}'),
      replace('potion.grey', 'a bubbly grey potion', 'a bubbly grey potion {fg:248}(flesh armor){reset}'),
      replace('potion.red', 'a bubbly red potion', 'a bubbly red potion {fg:248}(cure blind){reset}'),
      replace('potion.white', 'a bubbly white potion', 'a bubbly white potion {fg:248}(sanctuary){reset}'),
    ],
  },

  // ── HERB LABELS ──────────────────────────────────────────────────
  // tintin lines 192-209.
  {
    id: 'herb_labels',
    category: 'labels',
    name: 'Herb labels',
    description: 'Spell-name annotations for herbs from highlights.tin 192-209.',
    defaultEnabled: true,
    triggers: [
      replace('herb.purple_seaweed', 'a dried purple seaweed', 'a dried purple seaweed {fg:248}(fly){reset}'),
      replace('herb.mandrake', 'a mandrake root', 'a mandrake root {fg:248}(stone skin){reset}'),
      replace('herb.red_herb', 'a small red herb', 'a small red herb {fg:248}(detect invis){reset}'),
      replace('herb.magenta', 'some Magenta Leaves', 'some Magenta Leaves {fg:248}(frenzy){reset}'),
      replace('herb.cinnamon', 'some cinnamon', 'some cinnamon {fg:248}(armor){reset}'),
      replace('herb.damiana', 'some damiana leaves', 'some damiana leaves {fg:248}(cure serious){reset}'),
      replace('herb.dark_black', 'some dark black leaves', 'some dark black leaves {fg:248}(sanctuary){reset}'),
      replace('herb.catnip', 'some dried catnip', 'some dried catnip {fg:248}(frenzy){reset}'),
      replace('herb.raspberry', 'some fermenting raspberry leaves', 'some fermenting raspberry leaves {fg:248}(shield){reset}'),
      replace('herb.opium', 'some finely cut opium', 'some finely cut opium {fg:248}(frenzy){reset}'),
      replace('herb.ginger', 'some ginger', 'some ginger {fg:248}(faerie fog){reset}'),
      replace('herb.greyish', 'some greyish herbs', 'some greyish herbs {fg:248}(bless){reset}'),
      replace('herb.mugwort', 'some mugwort', 'some mugwort {fg:248}(slow){reset}'),
      replace('herb.mullein', 'some mullein', 'some mullein {fg:248}(pass door){reset}'),
      replace('herb.coca', 'some purified coca', 'some purified coca {fg:248}(endorphins){reset}'),
      replace('herb.rosemary', 'some rosemary', 'some rosemary {fg:248}(protection){reset}'),
      replace('herb.sand_leaves', 'some sand colored leaves', 'some sand colored leaves {fg:248}(stone skin){reset}'),
      replace('herb.spearmint', 'some spearmint', 'some spearmint {fg:248}(giant strength){reset}'),
    ],
  },
];

export function presetTriggers(preset: Preset): TriggerRecord[] {
  return preset.triggers.map((t) => ({ ...t, preset: preset.id }));
}

export function defaultEnabledIds(): string[] {
  return PRESETS.filter((p) => p.defaultEnabled).map((p) => p.id);
}

export function presetById(id: string): Preset | undefined {
  return PRESETS.find((p) => p.id === id);
}
