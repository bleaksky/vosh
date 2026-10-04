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
  | 'labels'
  | 'chat'
  | 'world';

export interface Preset {
  id: string;
  category: PresetCategory;
  name: string;
  description: string;
  defaultEnabled: boolean;
  triggers: Omit<TriggerRecord, 'preset'>[];
}

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
    patterns: [{ pattern, enabled: true }],
    priority,
    enabled: true,
    actions: [{ kind: 'highlight', style }],
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
    patterns: [{ pattern, enabled: true }],
    priority,
    enabled: true,
    actions: [{ kind: 'replace', template: colorize(template) }],
  };
}

// Color shorthands. The trigger backend's HighlightStyle.fg only
// accepts the 16 ANSI named colors (black, red, green, ...,
// bright_white). The terminal theme maps those to the Kanso palette,
// so using named colors here means presets pick up whatever theme is
// active.
const GREEN: HighlightStyle = { fg: 'bright_green' };
const RED: HighlightStyle = { fg: 'bright_red', bold: true };

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

// The weather blue, a true color apart from the theme's cyan and blue, the
// same on every theme. Keep highlight colors readable darkens it on a light
// theme, where it would fade (crates/automation/src/trigger/readable.rs).
const WEATHER_BLUE = '{#8fa7d9}';

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

// Token table now lives in src/lib/colorTokens.ts so the trigger form
// editor can use the same grammar (and the inverse).

export const PRESETS: Preset[] = [
  // ── Healing & Cure ────────────────────────────────────────────────
  {
    id: 'healing_basics',
    category: 'healing',
    name: 'Cures and heals',
    description: 'Turns cure and heal lines green so you spot them at a glance.',
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
    name: 'Parries, dodges, and blocks',
    description: 'Dims routine parries, dodges, and blocks to the dark grey your TinTin++ uses.',
    defaultEnabled: true,
    triggers: [
      // Generic "You dodge X." / "You parry X." — matches the bare
      // form in highlights.tin line 97. Lower priority so the more
      // specific "block / dual parry / reverse" replacements below
      // can win on lines they uniquely identify.
      replace('def.dodge_or_parry', '^You (?:dodge|parry) .+\\.$', '{fg:240}$0{reset}'),
      // Redirect-momentum counter (ends in `!` so it's not caught by
      // the generic period-anchored pattern above). Same dim treatment
      // as a normal dodge.
      replace('def.redirect_momentum', '^You .+ and redirect the momentum!$', '{fg:240}$0{reset}'),
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
      replace('def.block_shield', '^You block .+ with your shield\\.$', '{fg:240}$0{reset}', 6),
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
      replace('def.dual_parry', '^You dual parry .+ attack\\.$', '{fg:240}$0{reset}', 6),
      replace('def.reverse', '^You reverse .+ attack.*\\.$', '{fg:240}$0{reset}', 6),
      // Stagger out of attack (highlights.tin line 95).
      replace('def.stagger', '^You stagger wildly out of .+ attack\\.$', '{fg:240}$0{reset}', 6),
      replace(
        'def.swing_through',
        '^You swing right through .+ blurred image\\.$',
        '{fg:240}$0{reset}',
        6,
      ),
      replace('def.misses', '^.+ swings wildly and misses you by a mile\\.$', '{fg:240}$0{reset}'),
      replace('def.shadows_envelop', '^Shadows envelop .+\\.$', '{fg:253}$0{reset}'),
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
      replace('def.giant_blade', '^The giant blade deflects .+ attack\\.$', '{fg:240}$0{reset}'),
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
    defaultEnabled: true,
    triggers: [
      // Visual recolor + auto-rearm send, demonstrating the
      // multi-action support. Mirrors the user's tintin #ACTION at
      // line 109 that does `get 1.;wield 1.` on disarm. Fires
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
            template: colorize(
              '{bold_red}##{reset} {fg:178}$1 disarms you and sends your SECONDARY weapon flying!{reset}',
            ),
          },
          { kind: 'send', template: 'get 1.;wield 1.' },
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
            template: colorize(
              '{bold_red}##{reset} {fg:178}$1 disarms you and sends your PRIMARY weapon flying!{reset}',
            ),
          },
          { kind: 'send', template: 'get 1.;wield 1.' },
        ],
      },
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
  // Highlight the line bold-bright-red AND fire the same auto-rearm
  // the user's tintin #ACTION (line 124) does. Both run on the same
  // trigger via the multi-actions support.
  {
    id: 'terror_events',
    category: 'events',
    // From highlights.tin line 124.
    name: 'Terror weapon drop',
    description: 'Turns the line bold red, then picks up your weapon and wields it.',
    defaultEnabled: true,
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
          { kind: 'highlight', style: RED },
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
      'Colors the damage verb amber in lines that start with Your, so your hits stand out ' +
      'and the rest of the line keeps its color.',
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
    name: 'Damage to you',
    description:
      'Dims lines where something hits you to grey, with the damage verb in red and ' +
      'misses in pale cyan.',
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
    // From highlights.tin lines 170 to 174.
    name: 'Gold, experience, and levels',
    description: 'Marks the gold, experience, levels, and skills you gain.',
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
      // The game prints the level and what you gain on two lines
      // (update.c gain_exp and advance_level), with hit point and
      // practice singular when one, so each line keeps its own words.
      replace('loot.level', '^You raise a level!!$', '{fg:120}You raise a level!!{reset}'),
      replace(
        'loot.level_gain',
        '^You gain:  (\\d+)/(\\d+) hit point(s?), (\\d+)/(\\d+) mana, (\\d+)/(\\d+) move, and (\\d+) practice(s?)\\.$',
        '{fg:120}You gain:  {fg:230}$1{fg:120}/$2 hit point$3, {fg:230}$4{fg:120}/$5 mana, {fg:230}$6{fg:120}/$7 move, and {fg:230}$8{fg:120} practice$9.{reset}',
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
    // From highlights.tin lines 180 to 186.
    name: 'Potion labels',
    description: 'Adds the spell a potion casts after its name.',
    defaultEnabled: true,
    triggers: [
      replace(
        'potion.brown',
        'a bubbly brown potion',
        'a bubbly brown potion {fg:248}(cure serious){reset}',
      ),
      replace(
        'potion.clear',
        'a bubbly clear potion',
        'a bubbly clear potion {fg:248}(invisibility){reset}',
      ),
      replace(
        'potion.crimson',
        'a bubbly crimson potion',
        'a bubbly crimson potion {fg:248}(fireball){reset}',
      ),
      replace(
        'potion.green',
        'a bubbly green potion',
        'a bubbly green potion {fg:248}(haste){reset}',
      ),
      replace(
        'potion.grey',
        'a bubbly grey potion',
        'a bubbly grey potion {fg:248}(flesh armor){reset}',
      ),
      replace(
        'potion.red',
        'a bubbly red potion',
        'a bubbly red potion {fg:248}(cure blind){reset}',
      ),
      replace(
        'potion.white',
        'a bubbly white potion',
        'a bubbly white potion {fg:248}(sanctuary){reset}',
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
    description: 'Adds the spell an herb casts after its name.',
    defaultEnabled: true,
    triggers: [
      replace(
        'herb.purple_seaweed',
        'a dried purple seaweed',
        'a dried purple seaweed {fg:248}(fly){reset}',
      ),
      replace('herb.mandrake', 'a mandrake root', 'a mandrake root {fg:248}(stone skin){reset}'),
      replace(
        'herb.red_herb',
        'a small red herb',
        'a small red herb {fg:248}(detect invis){reset}',
      ),
      replace('herb.magenta', 'some Magenta Leaves', 'some Magenta Leaves {fg:248}(frenzy){reset}'),
      replace('herb.cinnamon', 'some cinnamon', 'some cinnamon {fg:248}(armor){reset}'),
      replace(
        'herb.damiana',
        'some damiana leaves',
        'some damiana leaves {fg:248}(cure serious){reset}',
      ),
      replace(
        'herb.dark_black',
        'some dark black leaves',
        'some dark black leaves {fg:248}(sanctuary){reset}',
      ),
      replace('herb.catnip', 'some dried catnip', 'some dried catnip {fg:248}(frenzy){reset}'),
      replace(
        'herb.raspberry',
        'some fermenting raspberry leaves',
        'some fermenting raspberry leaves {fg:248}(shield){reset}',
      ),
      replace(
        'herb.opium',
        'some finely cut opium',
        'some finely cut opium {fg:248}(frenzy){reset}',
      ),
      replace('herb.ginger', 'some ginger', 'some ginger {fg:248}(faerie fog){reset}'),
      replace('herb.greyish', 'some greyish herbs', 'some greyish herbs {fg:248}(bless){reset}'),
      replace('herb.mugwort', 'some mugwort', 'some mugwort {fg:248}(slow){reset}'),
      replace('herb.mullein', 'some mullein', 'some mullein {fg:248}(pass door){reset}'),
      replace('herb.coca', 'some purified coca', 'some purified coca {fg:248}(endorphins){reset}'),
      replace('herb.rosemary', 'some rosemary', 'some rosemary {fg:248}(protection){reset}'),
      replace(
        'herb.sand_leaves',
        'some sand colored leaves',
        'some sand colored leaves {fg:248}(stone skin){reset}',
      ),
      replace('herb.spearmint', 'some spearmint', 'some spearmint {fg:248}(giant strength){reset}'),
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
    defaultEnabled: true,
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
  // The look and the clock in four of the theme's terminal colors, as the
  // redesign mockups draw them, so they follow every theme. In Nord these
  // are the mockup colors exactly. The armies, things and people a room
  // lists match through the Room target, which the session gives only the
  // lines a look lists after its exits line, with the counts of people and
  // objects from the Room.Chars and Room.Items packets. The line of the
  // one you target with tar matches through the Your target match, in the
  // theme's bright red at a priority above the room yellow, so it stands
  // out from the rest of the room. The exits and room colors are base
  // colors, which fill only what the game left uncolored, so an aura, a
  // red [AFK] or a trap's red + keeps its own color. WiZNET (act_wiz.c
  // wiznet) turns its tag bold magenta, the mockup's mauve, where the game
  // sends it white and grey.
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
    defaultEnabled: true,
    triggers: [
      highlight('room.exits', EXITS_LINE, { fg: 'green', base: true }, 6),
      {
        ...highlight('room.contents', '^.+$', { fg: 'yellow', base: true }, 4),
        target: 'room',
      },
      {
        ...highlight('room.target', '^.+$', { fg: 'bright_red', base: true }, 5),
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
        actions: [{ kind: 'highlight', style: { fg: 'blue' } }],
      },
      {
        name: 'weather.change',
        patterns: WEATHER_CHANGES.map((line) => ({
          pattern: `^${escapeRegex(line)}$`,
          enabled: true,
        })),
        priority: 6,
        enabled: true,
        actions: [{ kind: 'replace', template: colorize(`${WEATHER_BLUE}$0{reset}`) }],
      },
      highlight('wiznet.tag', '^WiZNET\\b', { fg: 'magenta', bold: true }, 6),
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
