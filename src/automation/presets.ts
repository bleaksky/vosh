// The highlight preset library.
//
// Each preset is a named bundle of triggers, or of macros, a user can
// toggle under Automation, then Presets. Toggling on installs every
// trigger and macro in the bundle (tagged with the preset id so we can
// find them again); toggling off removes everything tagged with that id.
// User-authored triggers and macros are untouched either way.
//
// Patterns are POSIX-flavored regex compatible with Rust's `regex`
// crate. Captures use $1, $2 in Replace templates. Highlight actions
// take a HighlightStyle and wrap matched text with ANSI on the way to
// the terminal.
//
// Each preset names its colors once, by what they mark. A template or a
// highlight names a color by its key, and presetTriggers fills the
// keys, so one swatch reaches every trigger that uses it.
//
// Seeded from the user's `~/tintin/highlights.tin`. Categories are
// chosen so noise-heavy event groups (others' buff churn, others'
// recall) can be toggled independently from must-see ones (your own
// buffs falling, your own recall).

import type {
  HighlightStyle,
  Macro,
  NamedColor,
  TriggerAction,
  TriggerRecord,
  TriggerTarget,
} from '../ipc/automation';
import { colorize } from './colorTokens';

export type PresetCategory =
  | 'healing'
  | 'defensive'
  | 'disarm_buff'
  | 'events'
  | 'loot'
  | 'labels'
  | 'chat'
  | 'world'
  | 'movement';

export interface Preset {
  id: string;
  category: PresetCategory;
  name: string;
  description: string;
  /** The worlds Get started suggests the preset on, each by the name
   *  KNOWN_WORLDS in knownWorlds.ts gives it. Empty when no world
   *  suggests it. Get started and the Presets page read this one field.
   *  The presets step lists the suggestions outside Chat, and the Chat
   *  step lists the ones in it. */
  suggest: readonly string[];
  /** One to five lines the game prints that show what the preset does,
   *  each in the game's own words, with the place in the game's source it
   *  comes from beside it. A character or a number the game fills in comes
   *  from the repo fixtures. presets.test.ts runs each line through the
   *  preset's own triggers and checks the colors it paints. A preset that
   *  only binds macros changes no line, so it has none. */
  sample: readonly PresetSampleLine[];
  /** The colors the preset paints, each once under a key its templates
   *  and highlights name, in the order its card lists the swatches. Empty
   *  when it paints none. */
  colors: Readonly<Record<string, PresetColor>>;
  triggers: PresetTrigger[];
  /** The keys the preset binds, each with the command it sends, in the
   *  order the preset's card lists them. Absent when it binds none. */
  macros?: readonly Omit<Macro, 'preset'>[];
}

/** A color a preset paints, named by what it marks. */
export interface PresetColor {
  /** What the color marks, as its swatch names it. */
  label: string;
  /** The color the preset ships, as a color token without its braces. A
   *  theme color by name, as bright_green or bold_red, which follows the
   *  theme, one of the 256 fixed colors as fg:178, or a true color as
   *  #8fa7d9. */
  token: string;
  /** Where the color sits. A highlight holds only the theme's sixteen
   *  named colors, and a template takes any color. */
  sits: 'highlight' | 'template';
}

/** A highlight as a preset holds it, its text color named by a key of
 *  the preset's colors. */
type PresetHighlight = { kind: 'highlight'; style: Omit<HighlightStyle, 'fg'> & { fg: string } };

/** A trigger as a preset holds it. A Replace template names each color by
 *  its key in braces, as {line}, and a highlight names its color by key. */
export type PresetTrigger = Omit<TriggerRecord, 'preset' | 'actions'> & {
  actions: (Exclude<TriggerAction, { kind: 'highlight' }> | PresetHighlight)[];
};

/** A line of a preset's sample. */
export interface PresetSampleLine {
  /** The line, word for word as the game prints it, without the colors
   *  the game sends. Where the line quotes what a character says, a page
   *  that shows the sample draws the quoted words as a bar, and never as
   *  text, so a sample never puts words in anyone's mouth. */
  text: string;
  /** The name of the trigger of the preset the line shows. */
  shows: string;
  /** Where the line sits for a trigger that matches through a target.
   *  room for a line a room look lists after its exits line, and
   *  room_target for the line of the one you target among them. A plain
   *  line leaves it out. */
  target?: Extract<TriggerTarget, 'room' | 'room_target'>;
  /** Words of the line a page draws as bars, never as text: a number or
   *  a name the line could hold, so the sample shows its shape and
   *  claims no value. */
  bars?: readonly string[];
}

// The Forsaken Lands, as KNOWN_WORLDS in knownWorlds.ts names it. Get
// started suggests six presets there. The presets step lists five, each
// changing only how a line looks, and their samples show what most
// characters meet early, a room, a fight, a cure and experience. The Chat
// step lists Tells you send, which puts the tells you send in the chat
// pane.
const FORSAKEN_LANDS = 'The Forsaken Lands';

// Category names as Settings, Automation shows them over the presets.
export const PRESET_CATEGORIES: Record<PresetCategory, string> = {
  healing: 'Healing and cures',
  defensive: 'Defense',
  disarm_buff: 'Disarms and fading buffs',
  events: 'Combat and spell events',
  loot: 'Loot and progress',
  labels: 'Potion and herb labels',
  chat: 'Chat',
  world: 'Rooms, time and weather',
  movement: 'Movement',
};

// Helper to build a highlight trigger compactly. Default priority of 5
// matches the user's TinTin highlight priority so user-authored
// triggers at the same number stay in stable order.
function highlight(
  name: string,
  pattern: string,
  style: PresetHighlight['style'],
  priority = 5,
): PresetTrigger {
  return {
    name,
    patterns: [{ pattern, enabled: true }],
    priority,
    enabled: true,
    actions: [{ kind: 'highlight', style }],
  };
}

// Replace trigger whose template carries its own coloring, so the
// resulting line needs no separate highlight pass. presetTriggers turns
// its color keys and tokens into ANSI.
function replace(name: string, pattern: string, template: string, priority = 5): PresetTrigger {
  return {
    name,
    patterns: [{ pattern, enabled: true }],
    priority,
    enabled: true,
    actions: [{ kind: 'replace', template }],
  };
}

// The exits line a room look prints with autoexit on (act_info.c
// do_exits with "auto"). Each exit shows by its full name, in parentheses
// while closed, with a + where you see a trap, or the line reads none.
// The prompt's %e code prints single letters, `[Exits: N E (S) W]`, and a
// blind or misty prompt prints `[Exits: --- ]` or `[Exits: ??? ]`, so
// none of those match. The session opens a room look on the same pattern
// (EXITS_PATTERN in src-tauri/src/session/room_block.rs).
const EXITS_LINE = '^\\[Exits:(?: none|(?: \\(?\\+?(?:north|east|south|west|up|down)\\)?)+)\\]$';

// Every time of day message the game sends (update.c weather_update), the
// usual five and the six it sends instead while an immortal holds the
// land in eternal darkness. Each one is a line of its own, so each
// pattern is anchored at both ends and a say that quotes one stays plain.
const TIME_OF_DAY = [
  'The night is about to end.',
  'The day has begun.',
  'The sun rises in the east.',
  'The sun slowly disappears in the west.',
  'The night has begun.',
  'The darkness begets an unbearable chill.',
  'The night air is suffocating.',
  'Darkness covers the landscape.',
  'Your bones ache as the chilly air cuts through you.',
  'The night air burns as the chilly air dances across your body.',
  'The landscape remains grey and lifeless.',
];

// Every change in the weather the game tells you about outdoors. The first
// fourteen are sky_event_text in update.c, which weather_update sends to
// each player outside in a region whose sky changed, in bold white. Rain,
// sleet or snow and their storms follow the temperature where you stand.
// The last ten are weather_affect_room in update.c, which tells each
// player in an outdoor room when its blizzard, sandstorm, ice, mud or fog
// sets in or clears. Each one is a line of its own, so each pattern is
// anchored at both ends and a say that quotes one stays plain.
const WEATHER_CHANGES = [
  'The sky is getting cloudy.',
  'The clouds disappear.',
  'It starts to snow.',
  'It starts to sleet.',
  'It starts to rain.',
  'The snowstorm becomes a blizzard.',
  'The sleet turns into hail.',
  'Lightning flashes in the sky.',
  'The snow stopped.',
  'The sleet stopped.',
  'The rain stopped.',
  'The blizzard has slowed down.',
  'The hail stops and it starts sleeting.',
  'The lightning has stopped.',
  'A fierce blizzard descends, howling winds and blinding snow engulfing the area.',
  'The blizzard subsides, the winds dying down.',
  'The winds pick up violently, hurling sand in every direction.',
  'The sandstorm dies down, the air clearing.',
  'The cold bites deep as ice forms across every surface.',
  'The ice begins to thaw and melt away.',
  'The rain soaks into the ground, turning it to thick mud.',
  'The ground begins to dry out and firm up.',
  'A thick fog rolls in, shrouding the area.',
  'The fog lifts, revealing the surroundings once more.',
];

// `text` as a regex that matches it literally.
function escapeRegex(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

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

// The color token table lives in src/automation/colorTokens.ts, so the
// trigger form editor reads the same grammar and its inverse. A highlight
// holds only the sixteen named colors, which the terminal theme maps, so
// a preset that highlights follows whatever theme is active.

// A color of a preset that sits in a template, and one in a highlight.
const inTemplate = (label: string, token: string): PresetColor => ({
  label,
  token,
  sits: 'template',
});
const inHighlight = (label: string, token: NamedColor): PresetColor => ({
  label,
  token,
  sits: 'highlight',
});

export const PRESETS: Preset[] = [
  // ── Healing & Cure ────────────────────────────────────────────────
  {
    id: 'healing_basics',
    category: 'healing',
    name: 'Cures and heals',
    description: 'Turns cure and heal lines green so you spot them at a glance.',
    // The heal of cure critical (magic.c spell_cure_critical) and poison
    // wearing off (the poison row of skill_table in const.c).
    sample: [
      { text: 'You feel a lot better!', shows: 'cure.feel_lot_better' },
      { text: 'You feel less sick.', shows: 'cure.less_sick' },
    ],
    suggest: [FORSAKEN_LANDS],
    colors: { line: inHighlight('The line', 'bright_green') },
    triggers: [
      highlight('cure.feel_lot_better', 'You feel a lot better!$', { fg: 'line' }),
      highlight('cure.feel_better', 'You feel better\\.$', { fg: 'line' }),
      // The herb cure, as do_herb prints it in skills2.c.
      highlight('cure.feel_much_better', 'You feel much better\\.$', { fg: 'line' }),
      highlight('cure.righteous', 'You feel righteous\\.$', { fg: 'line' }),
      highlight('cure.less_sick', 'You feel less sick\\.$', { fg: 'line' }),
      highlight('cure.less_tired', 'You feel less tired\\.$', { fg: 'line' }),
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
    name: 'Parries, dodges, and blocks',
    description:
      'Dims routine parries, dodges, and blocks to dark grey, so the blows that land stand out.',
    // check_parry, check_dodge and check_shield_block in fight.c, against
    // a villager, mob 5287 in area/fortblac.are, which fixtures/room-colors
    // names by its short text.
    sample: [
      { text: "You parry a villager's attack.", shows: 'def.dodge_or_parry' },
      { text: "You dodge a villager's attack.", shows: 'def.dodge_or_parry' },
      { text: "You block a villager's attack with your shield.", shows: 'def.block_shield' },
    ],
    suggest: [],
    colors: {
      routine: inTemplate('Routine defenses', 'fg:240'),
      shadows: inTemplate('Shadows envelop', 'fg:253'),
    },
    triggers: [
      // Generic "You dodge X." / "You parry X." — matches the bare
      // form in highlights.tin line 97. Lower priority so the more
      // specific "block / dual parry / reverse" replacements below
      // can win on lines they uniquely identify.
      replace('def.dodge_or_parry', '^You (?:dodge|parry) .+\\.$', '{routine}$0{reset}'),
      // Redirect-momentum counter (ends in `!` so it's not caught by
      // the generic period-anchored pattern above). Same dim treatment
      // as a normal dodge.
      replace('def.redirect_momentum', '^You .+ and redirect the momentum!$', '{routine}$0{reset}'),
      // Shadow-blend evade — assassin/thief flavor defense, ends in
      // `!` like the redirect.
      replace(
        'def.shadows_evade',
        '^You blend into the shadows, evading .+!$',
        '{routine}$0{reset}',
      ),
      // Parry with hand specified (highlights.tin line 93).
      replace(
        'def.parry_hand',
        '^You parry .+ attack with your (?:first|second) hand\\.$',
        '{routine}$0{reset}',
        6,
      ),
      replace('def.block_shield', '^You block .+ with your shield\\.$', '{routine}$0{reset}', 6),
      replace(
        'def.block_weapon',
        '^You block .+ attack with your weapon\\.$',
        '{routine}$0{reset}',
        6,
      ),
      // Block and attempt to strike (highlights.tin line 89).
      replace(
        'def.block_attempt',
        '^You block .+ attack and attempt to strike at the brief opening\\.$',
        '{routine}$0{reset}',
        6,
      ),
      replace('def.dual_parry', '^You dual parry .+ attack\\.$', '{routine}$0{reset}', 6),
      replace('def.reverse', '^You reverse .+ attack.*\\.$', '{routine}$0{reset}', 6),
      // Stagger out of attack (highlights.tin line 95).
      replace('def.stagger', '^You stagger wildly out of .+ attack\\.$', '{routine}$0{reset}', 6),
      replace(
        'def.swing_through',
        '^You swing right through .+ blurred image\\.$',
        '{routine}$0{reset}',
        6,
      ),
      replace('def.misses', '^.+ swings wildly and misses you by a mile\\.$', '{routine}$0{reset}'),
      replace('def.shadows_envelop', '^Shadows envelop .+\\.$', '{shadows}$0{reset}'),
      replace(
        'def.terra_shield',
        '^Your Terra shield deflects the attack\\.$',
        '{routine}$0{reset}',
      ),
      // Faith save (highlights.tin line 99).
      replace(
        'def.faith',
        '^Your faith holding fast, you stop the blow with .+ power\\.$',
        '{routine}$0{reset}',
      ),
      // Giant blade deflect (highlights.tin line 88).
      replace('def.giant_blade', '^The giant blade deflects .+ attack\\.$', '{routine}$0{reset}'),
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
    // From highlights.tin lines 105 to 134.
    name: 'Disarms and fading buffs',
    description: 'Marks a disarm and a buff that wears off.',
    // disarm in skills.c, here by Maren, and sanctuary wearing off (the
    // sanctuary row of skill_table in const.c).
    sample: [
      { text: 'Maren disarms you and sends your weapon flying!', shows: 'disarm.primary' },
      { text: 'The protective aura around your body fades.', shows: 'buff.protective_aura' },
    ],
    suggest: [],
    colors: {
      mark: inTemplate('The ## mark', 'bold_red'),
      line: inTemplate('The line', 'fg:178'),
    },
    triggers: [
      // Visual recolor + auto-rearm send, demonstrating the
      // multi-action support. Mirrors the user's tintin #ACTION at
      // line 109 that does `get 1.;wield 1.` on disarm, with dual in
      // place of wield for the secondary weapon. Fires
      // unconditionally — when the attacker is "Someone" (blinded)
      // it still tries, which can pick up junk; that edge case is
      // the cost of not having lookarounds in the trigger regex.
      {
        name: 'disarm.secondary',
        patterns: [
          { pattern: '^(.+) disarms you and sends your secondary weapon flying!$', enabled: true },
        ],
        priority: 5,
        enabled: true,
        actions: [
          {
            kind: 'replace',
            template:
              '{mark}##{reset} {line}$1 disarms you and sends your SECONDARY weapon flying!{reset}',
          },
          // The off hand takes its weapon back with dual (do_second,
          // interp.c), where wield would swap out the primary.
          { kind: 'send', template: 'get 1.;dual 1.' },
        ],
      },
      {
        name: 'disarm.primary',
        patterns: [{ pattern: '^(.+) disarms you and sends your weapon flying!$', enabled: true }],
        priority: 5,
        enabled: true,
        actions: [
          {
            kind: 'replace',
            template:
              '{mark}##{reset} {line}$1 disarms you and sends your PRIMARY weapon flying!{reset}',
          },
          { kind: 'send', template: 'get 1.;wield 1.' },
        ],
      },
      replace(
        'buff.protective_shield',
        '^(.+) protective shield dissipates\\.$',
        '{mark}##{reset} {line}$1 protective shield dissipates.{reset}',
      ),
      replace(
        'buff.protective_aura',
        '^The protective aura around your body fades\\.$',
        '{mark}##{reset} {line}The protective aura around your body fades.{reset}',
      ),
      replace(
        'buff.stoneskin',
        '^The shards of metal protecting you fall to the ground\\.$',
        '{mark}##{reset} {line}The shards of metal protecting you fall to the ground.{reset}',
      ),
      replace(
        'buff.sanctuary',
        // Sanctuary wearing off someone else (const.c, its msg_off2).
        // Your own fade is buff.protective_aura.
        '^The protective aura around (.+) fades\\.$',
        '{mark}##{reset} {line}The protective aura around $1 fades.{reset}',
      ),
      replace(
        'buff.spell_turning',
        '^Your shield of spell turning collapses\\.$',
        '{mark}##{reset} {line}Your shield of spell turning collapses.{reset}',
      ),
    ],
  },

  // ── Terror weapon drop ────────────────────────────────────────────
  // Highlight the line bold-bright-red AND fire the same auto-rearm
  // the user's tintin #ACTION (line 124) does. Both run on the same
  // trigger via the multi-actions support.
  {
    id: 'terror_events',
    category: 'events',
    // From highlights.tin line 124.
    name: 'Terror weapon drop',
    description: 'Turns the line bold red, then picks up your weapon and wields it.',
    // multi_hit in fight.c.
    sample: [
      {
        text: 'Filled with terror, your weapon slips through your slippery fingers.',
        shows: 'terror.drop',
      },
    ],
    suggest: [],
    colors: { line: inHighlight('The line', 'bright_red') },
    triggers: [
      {
        name: 'terror.drop',
        patterns: [
          {
            pattern: '^Filled with terror, your weapon slips through your slippery fingers\\.$',
            enabled: true,
          },
        ],
        priority: 5,
        enabled: true,
        actions: [
          { kind: 'highlight', style: { fg: 'line', bold: true } },
          { kind: 'send', template: 'get 1.;wield 1.' },
        ],
      },
    ],
  },

  // ── Outgoing damage ───────────────────────────────────────────────
  // Every "Your <attack> <VERB> <target><.|!>" line gets the verb
  // recolored to a burnt-amber tone. Matches both lowercase verbs
  // (hits, scratches) and uppercase ones (LACERATES, DISINTEGRATES).
  {
    id: 'combat_outgoing',
    category: 'events',
    name: 'Your damage verbs',
    description:
      'Colors the damage verb amber in lines that start with You, so your hits stand out ' +
      'and the rest of the line keeps its color.',
    // The top hit in dam_message in fight.c, on a villager, mob 5287 in
    // area/fortblac.are.
    sample: [
      {
        text: 'You do UNSPEAKABLE things to a villager!',
        shows: 'combat.outgoing',
        bars: ['a villager'],
      },
    ],
    suggest: [FORSAKEN_LANDS],
    colors: {
      line: inTemplate('The rest of the line', 'fg:253'),
      verb: inTemplate('The damage verb', 'fg:214'),
      miss: inTemplate('A miss', 'fg:152'),
    },
    triggers: [
      // Mirrors the TinTin `You%1` form so both "Your kick LACERATES
      // X" and "You LACERATE X" / "You miss X" lines fire — the
      // optional `(?:r .+?)?` lets group 1 capture "You " or "Your
      // <attack> ".
      replace(
        'combat.outgoing',
        `^(You(?:r .+?)? )(${DAMAGE_VERB_WRAPPED})( .+[!.])$`,
        '{line}$1{reset}{verb}$2{reset}{line}$3{reset}',
        7,
      ),
      // Outgoing miss — `<aee>` pale cyan on the verb, `<g21>` body
      // (matching the damage-hit body color).
      replace(
        'combat.outgoing_miss',
        '^(You(?:r .+?)? )(misses|miss)( .+[!.])$',
        '{line}$1{reset}{miss}$2{reset}{line}$3{reset}',
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
    name: 'Damage to you',
    description:
      'Dims lines where something hits you to grey, with the damage verb in red and ' +
      'misses in pale cyan.',
    // A hit and a miss on you in dam_message in fight.c, from a villager,
    // mob 5287 in area/fortblac.are, whose attack is a punch. Bars stand
    // for the attacker and the attack.
    sample: [
      {
        text: "A villager's punch decimates you!",
        shows: 'combat.incoming',
        bars: ["A villager's", 'punch'],
      },
      {
        text: "A villager's punch misses you.",
        shows: 'combat.incoming_miss',
        bars: ["A villager's", 'punch'],
      },
    ],
    suggest: [FORSAKEN_LANDS],
    colors: {
      line: inTemplate('The rest of the line', 'fg:244'),
      verb: inTemplate('The damage verb', 'fg:210'),
      miss: inTemplate('A miss', 'fg:152'),
    },
    triggers: [
      replace(
        'combat.incoming',
        `^(.+? )(${DAMAGE_VERB_WRAPPED})( you[!.])$`,
        '{line}$1{reset}{verb}$2{reset}{line}$3{reset}',
        7,
      ),
      // Incoming miss — `<aee>` pale cyan on the verb, `<g12>` body.
      replace(
        'combat.incoming_miss',
        '^(.+? )(misses|miss)( you[!.])$',
        '{line}$1{reset}{miss}$2{reset}{line}$3{reset}',
        7,
      ),
    ],
  },

  // ── LOOT & PROGRESSION ────────────────────────────────────────────
  // tintin lines 170-174. Numbers in near-white, body in mid grey.
  {
    id: 'loot_progression',
    category: 'loot',
    // From highlights.tin lines 170 to 174.
    name: 'Gold, experience, and levels',
    description: 'Marks the gold, experience, levels, and skills you gain.',
    // group_gain in fight.c, with the experience to the next level of
    // fixtures/gmcp/aabahran group-info.gmcp, 1250, then the line
    // check_improve prints at skills.c:1442. Dagger is a skill of
    // skill_table in const.c, and bars stand for the number and the skill.
    sample: [
      { text: 'You receive 1250 experience points.', shows: 'loot.xp', bars: ['1250'] },
      { text: 'You have become better at dagger!', shows: 'loot.skill_up', bars: ['dagger'] },
    ],
    suggest: [FORSAKEN_LANDS],
    colors: {
      gain: inTemplate('What you gain', 'fg:230'),
      progress: inTemplate('Skill and level lines', 'fg:120'),
      gold: inTemplate('The gold line', 'fg:249'),
      xp: inTemplate('The experience line', 'fg:248'),
    },
    triggers: [
      replace(
        'loot.gold',
        '^You get (\\d+) gold coins from (.+)\\.$',
        '{gold}You get {gain}$1 {gold}gold coins from $2.{reset}',
      ),
      replace(
        'loot.skill_up',
        // check_improve in skills.c adds the percent you reach, as in
        // [78%]. A song gain (check_improve_song) prints none.
        '^You have become better at (.+)!( \\[\\d+%\\])?$',
        '{progress}You have become better at {gain}$1{progress}!$2{reset}',
      ),
      // The game prints the level and what you gain on two lines
      // (update.c gain_exp and advance_level), with hit point and
      // practice singular when one, so each line keeps its own words.
      replace('loot.level', '^You raise a level!!$', '{progress}You raise a level!!{reset}'),
      replace(
        'loot.level_gain',
        '^You gain:  (\\d+)/(\\d+) hit point(s?), (\\d+)/(\\d+) mana, (\\d+)/(\\d+) move, and (\\d+) practice(s?)\\.$',
        '{progress}You gain:  {gain}$1{progress}/$2 hit point$3, {gain}$4{progress}/$5 mana, {gain}$6{progress}/$7 move, and {gain}$8{progress} practice$9.{reset}',
      ),
      replace(
        'loot.xp',
        '^You receive (\\d+) experience points\\.$',
        '{xp}You receive {gain}$1 {xp}experience points.{reset}',
      ),
    ],
  },

  // ── POTION LABELS ────────────────────────────────────────────────
  // tintin lines 180-186. Appends a grey `(spell)` parenthetical
  // to each potion description.
  {
    id: 'potion_labels',
    category: 'labels',
    // From highlights.tin lines 180 to 186, with each spell checked
    // against the bubbly potions do_brew makes in skills5.c.
    name: 'Potion labels',
    description: 'Adds the spell a potion casts after its name.',
    // do_brew in skills5.c, which makes a pink potion of cure light from
    // food, and do_quaff in act_obj.c.
    sample: [
      { text: 'You brew a bubbly pink potion from a large kettle!', shows: 'potion.pink' },
      { text: 'You quaff a bubbly pink potion.', shows: 'potion.pink' },
    ],
    suggest: [],
    colors: { spell: inTemplate('The spell', 'fg:248') },
    triggers: [
      replace('potion.blue', 'a bubbly blue potion', 'a bubbly blue potion {spell}(armor){reset}'),
      replace(
        'potion.brown',
        'a bubbly brown potion',
        'a bubbly brown potion {spell}(cure serious){reset}',
      ),
      replace(
        'potion.clear',
        'a bubbly clear potion',
        'a bubbly clear potion {spell}(invisibility){reset}',
      ),
      replace(
        'potion.crimson',
        'a bubbly crimson potion',
        'a bubbly crimson potion {spell}(frenzy){reset}',
      ),
      replace(
        'potion.green',
        'a bubbly green potion',
        'a bubbly green potion {spell}(haste){reset}',
      ),
      replace('potion.grey', 'a bubbly grey potion', 'a bubbly grey potion {spell}(bless){reset}'),
      replace(
        'potion.orange',
        'a bubbly orange potion',
        'a bubbly orange potion {spell}(fireball){reset}',
      ),
      replace(
        'potion.pink',
        'a bubbly pink potion',
        'a bubbly pink potion {spell}(cure light){reset}',
      ),
      replace(
        'potion.red',
        'a bubbly red potion',
        'a bubbly red potion {spell}(cure blind){reset}',
      ),
      replace(
        'potion.white',
        'a bubbly white potion',
        'a bubbly white potion {spell}(sanctuary){reset}',
      ),
    ],
  },

  // ── HERB LABELS ──────────────────────────────────────────────────
  // tintin lines 192-209.
  {
    id: 'herb_labels',
    category: 'labels',
    // From highlights.tin lines 192 to 209.
    name: 'Herb labels',
    description: 'Adds the spell an herb casts when you smoke it.',
    // do_smoke in act_obj.c with object 1147 in area/hamlet.are, whose
    // smoke casts protection.
    sample: [{ text: 'You light some rosemary and begin to smoke it.', shows: 'herb.rosemary' }],
    suggest: [],
    colors: { spell: inTemplate('The spell', 'fg:248') },
    triggers: [
      replace(
        'herb.purple_seaweed',
        'a dried purple seaweed',
        'a dried purple seaweed {spell}(fly){reset}',
      ),
      replace('herb.mandrake', 'a mandrake root', 'a mandrake root {spell}(stone skin){reset}'),
      replace('herb.red_herb', 'a small red herb', 'a small red herb {spell}(detect invis){reset}'),
      replace('herb.magenta', 'some Magenta Leaves', 'some Magenta Leaves {spell}(frenzy){reset}'),
      replace('herb.cinnamon', 'some cinnamon', 'some cinnamon {spell}(armor){reset}'),
      replace(
        'herb.damiana',
        'some damiana leaves',
        'some damiana leaves {spell}(cure serious){reset}',
      ),
      replace(
        'herb.dark_black',
        'some dark black leaves',
        'some dark black leaves {spell}(sanctuary){reset}',
      ),
      replace('herb.catnip', 'some dried catnip', 'some dried catnip {spell}(frenzy){reset}'),
      replace(
        'herb.raspberry',
        'some fermenting raspberry leaves',
        'some fermenting raspberry leaves {spell}(shield){reset}',
      ),
      replace(
        'herb.opium',
        'some finely cut opium',
        'some finely cut opium {spell}(frenzy){reset}',
      ),
      replace('herb.ginger', 'some ginger', 'some ginger {spell}(faerie fog){reset}'),
      replace('herb.greyish', 'some greyish herbs', 'some greyish herbs {spell}(bless){reset}'),
      replace('herb.mugwort', 'some mugwort', 'some mugwort {spell}(slow){reset}'),
      replace('herb.mullein', 'some mullein', 'some mullein {spell}(pass door){reset}'),
      replace(
        'herb.coca',
        'some purified coca',
        'some purified coca {spell}(endorphin rush){reset}',
      ),
      replace('herb.rosemary', 'some rosemary', 'some rosemary {spell}(protection){reset}'),
      replace(
        'herb.sand_leaves',
        'some sand colored leaves',
        'some sand colored leaves {spell}(stone skin){reset}',
      ),
      replace('herb.spearmint', 'some spearmint', 'some spearmint {spell}(giant strength){reset}'),
    ],
  },

  // ── CHAT ─────────────────────────────────────────────────────────
  // The game sends a Comm.Channel packet for a tell you receive and
  // none for one you send, so this routes the line it prints instead
  // (languages.c compose_tell): `You tell Tolliver 'text'`, with
  // ` in Elvish` before the quote outside common, and `You project to`
  // for a telepath. The chat store reads the line as your side of the
  // tell and skips `You tell your group`, whose gtell packet the pane
  // already has (chatStore.ts parseRoutedLine).
  {
    id: 'sent_tells',
    category: 'chat',
    name: 'Tells you send',
    description: 'Puts each tell you send in the chat pane, beside the ones you get.',
    // compose_tell in languages.c, to Tolliver. The tell quotes a time of
    // day line, as the says and tells in fixtures/room-colors do, so the
    // sample holds no words a player wrote. Get started and the Looks like
    // row still draw the quoted words as a bar, the span parseRoutedLine
    // in chatStore.ts gives as its text.
    sample: [{ text: "You tell Tolliver 'The day has begun.'", shows: 'chat.sent_tells' }],
    suggest: [FORSAKEN_LANDS],
    colors: {},
    triggers: [
      {
        name: 'chat.sent_tells',
        patterns: [{ pattern: "^You (tell|project to) .+ '", enabled: true }],
        priority: 5,
        enabled: true,
        actions: [{ kind: 'route', pane: 'tell' }],
      },
    ],
  },

  // ── ROOMS, TIME AND WEATHER ──────────────────────────────────────
  // The look and the clock in four of the theme's terminal colors, so they
  // follow every theme. The armies, things and people a room lists match
  // through the Room target, which the session gives only the lines a look
  // lists after its exits line, with the counts of people and objects from
  // the Room.Chars and Room.Items packets. The line of the one you target
  // with tar matches through the Your target match, in the theme's bright
  // red at a priority above the room yellow, so it stands out from the rest
  // of the room. The exits and room colors are base colors, which fill only
  // what the game left uncolored, so an aura, a red [AFK] or a trap's red +
  // keeps its own color. WiZNET (act_wiz.c wiznet) turns its tag bold
  // magenta, where the game sends it white and grey.
  // A highlight draws over the text it matches alone, so the time and the
  // message after the tag keep the colors the game sent, such as the bold
  // red of a corrupted pfile alert. A change in the weather redraws its
  // whole line in the weather blue, a true color, in place of the bold
  // white the game sends with the sky's changes. The id keeps the name the
  // preset had before it colored the weather, so the profiles that have it
  // on keep it on.
  {
    id: 'room_and_time',
    category: 'world',
    name: 'Room, time and weather colors',
    description:
      'Colors the exits green, what is in the room yellow, your target in the room bright ' +
      'red, the time of day blue, a change in the weather pale blue, and the WiZNET tag ' +
      'magenta.',
    // The look in fixtures/room-colors/looks.json where your target, a
    // villager, fights a player who names it in their own line. Its exits
    // line, the villager and Maren as char_to_char in act_info.c prints
    // them, the time of day message, and a change in the weather from
    // sky_event_text in update.c.
    sample: [
      { text: '[Exits: south]', shows: 'room.exits' },
      {
        text: 'A villager is here, fighting Maren.',
        shows: 'room.target',
        target: 'room_target',
      },
      {
        text: 'Maren is here, fighting a villager.',
        shows: 'room.contents',
        target: 'room',
      },
      { text: 'The day has begun.', shows: 'time.of_day' },
      { text: 'It starts to rain.', shows: 'weather.change' },
    ],
    suggest: [FORSAKEN_LANDS],
    colors: {
      exits: inHighlight('Exits', 'green'),
      contents: inHighlight('What is in the room', 'yellow'),
      target: inHighlight('Your target', 'bright_red'),
      time: inHighlight('Time of day', 'blue'),
      // The weather blue, a true color apart from the theme's cyan and
      // blue, the same on every theme. Keep highlight colors readable
      // darkens it on a light theme, where it would fade
      // (crates/automation/src/trigger/readable.rs).
      weather: inTemplate('Weather change', '#8fa7d9'),
      wiznet: inHighlight('WiZNET tag', 'magenta'),
    },
    triggers: [
      highlight('room.exits', EXITS_LINE, { fg: 'exits', base: true }, 6),
      {
        ...highlight('room.contents', '^.+$', { fg: 'contents', base: true }, 4),
        target: 'room',
      },
      {
        ...highlight('room.target', '^.+$', { fg: 'target', base: true }, 5),
        target: 'room_target',
      },
      {
        name: 'time.of_day',
        patterns: TIME_OF_DAY.map((line) => ({
          pattern: `^${escapeRegex(line)}$`,
          enabled: true,
        })),
        priority: 6,
        enabled: true,
        actions: [{ kind: 'highlight', style: { fg: 'time' } }],
      },
      {
        name: 'weather.change',
        patterns: WEATHER_CHANGES.map((line) => ({
          pattern: `^${escapeRegex(line)}$`,
          enabled: true,
        })),
        priority: 6,
        enabled: true,
        actions: [{ kind: 'replace', template: '{weather}$0{reset}' }],
      },
      highlight('wiznet.tag', '^WiZNET\\b', { fg: 'wiznet', bold: true }, 6),
    ],
  },
  // The six directions the game has, on the numpad as the arrows sit
  // there, with up on 9 and down on 3. The game has no diagonal exits,
  // so 7, 1 and 5 stay free. Each macro sends the one letter form, which
  // read_from_buffer in the game's comm.c never counts toward its spam
  // limit. The keys come from event.code, so NumLock leaves them as they
  // are. A key one of your macros uses stays yours, and Rust holds the
  // preset's macro on it off (hold_taken_keys in
  // src-tauri/src/loadouts/presets.rs). Get started suggests it on no
  // world.
  {
    id: 'numpad_movement',
    category: 'movement',
    name: 'Numpad movement',
    description: 'Walk with the numpad. The game has six directions, so 7, 1 and 5 stay free.',
    suggest: [],
    sample: [],
    colors: {},
    triggers: [],
    macros: [
      { key: 'Numpad8', command: 'n' },
      { key: 'Numpad6', command: 'e' },
      { key: 'Numpad2', command: 's' },
      { key: 'Numpad4', command: 'w' },
      { key: 'Numpad9', command: 'u' },
      { key: 'Numpad3', command: 'd' },
    ],
  },
];

// The presets an empty enabled_presets list turns on, which are the
// eleven the library held when new presets began to ship off. The list
// is frozen. A preset added later starts off, and an empty list, the
// value every profile holds until you change a preset, keeps the meaning
// it had when it was saved. PRESETS_ON_BY_DEFAULT in
// src-tauri/src/loadouts/presets.rs mirrors it, and a test there reads
// this list.
export const PRESETS_ON_BY_DEFAULT: readonly string[] = [
  'healing_basics',
  'defensive_combat',
  'disarm_buff_fade',
  'terror_events',
  'combat_outgoing',
  'combat_incoming',
  'loot_progression',
  'potion_labels',
  'herb_labels',
  'sent_tells',
  'room_and_time',
];

/** The triggers of `preset` as the store holds them, each color key
 *  filled from `colors`, yours by key, over the preset's own. With none of
 *  yours they are the triggers the preset ships. */
export function presetTriggers(
  preset: Preset,
  colors: Readonly<Record<string, string>> = {},
): TriggerRecord[] {
  return preset.triggers.map((t) => fillColors(preset, t, colors));
}

/** `trigger`, one of `preset`'s with your edits laid over it or as it
 *  ships, as the store holds it. Each color key fills from `colors`,
 *  yours by key, over the preset's own, and a color of your own in place
 *  of a key stays as it is. A mark the preset paints bold keeps its bold
 *  in any color you give it. */
export function fillColors(
  preset: Preset,
  trigger: PresetTrigger,
  colors: Readonly<Record<string, string>> = {},
): TriggerRecord {
  const isKey = (key: string) => Object.hasOwn(preset.colors, key);
  const token = (key: string) => colors[key] ?? preset.colors[key].token;
  const inBraces = (key: string) => {
    const own = colors[key];
    const bold =
      own !== undefined && !own.startsWith('bold_') && preset.colors[key].token.startsWith('bold_');
    return `${bold ? '{bold}' : ''}{${token(key)}}`;
  };
  const fill = (template: string) =>
    colorize(
      template.replace(/\{([a-z_]+)\}/g, (m, key: string) => (isKey(key) ? inBraces(key) : m)),
    );
  return {
    ...trigger,
    actions: trigger.actions.map((a): TriggerAction => {
      if (a.kind === 'highlight') {
        const fg = isKey(a.style.fg) ? token(a.style.fg) : a.style.fg;
        return { ...a, style: { ...a.style, fg: fg as NamedColor } };
      }
      return a.kind === 'replace' ? { ...a, template: fill(a.template) } : a;
    }),
    preset: preset.id,
  };
}

export function presetMacros(preset: Preset): Macro[] {
  return (preset.macros ?? []).map((m) => ({ ...m, preset: preset.id }));
}

/** The presets on by default, in library order. */
export function defaultEnabledIds(): string[] {
  return PRESETS.filter((p) => PRESETS_ON_BY_DEFAULT.includes(p.id)).map((p) => p.id);
}

/** The name of every trigger in the library, each preset on or off. An
 *  import from another client never replaces one of these. */
export function presetTriggerNames(): string[] {
  return PRESETS.flatMap((p) => p.triggers.map((t) => t.name));
}

export function presetById(id: string): Preset | undefined {
  return PRESETS.find((p) => p.id === id);
}
