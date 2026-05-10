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

export type PresetCategory =
  | 'healing'
  | 'defensive'
  | 'buff_falls_self'
  | 'buff_falls_others'
  | 'events';

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
  buff_falls_self: 'Your Buffs Fading',
  buff_falls_others: "Others' Buffs Fading",
  events: 'Combat & Spell Events',
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
const SOFT_GREEN: HighlightStyle = { fg: 'green' };
const RED: HighlightStyle = { fg: 'bright_red', bold: true };

// Named-color templating for Replace actions. Authors write
// `{bold_red}## {red}$0{reset}` instead of raw ANSI escape codes.
// `{fg:N}` and `{bg:N}` cover 256-color.
const COLOR_TOKENS: Record<string, string> = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  underline: '\x1b[4m',
  black: '\x1b[30m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  white: '\x1b[37m',
  bright_black: '\x1b[90m',
  bright_red: '\x1b[91m',
  bright_green: '\x1b[92m',
  bright_yellow: '\x1b[93m',
  bright_blue: '\x1b[94m',
  bright_magenta: '\x1b[95m',
  bright_cyan: '\x1b[96m',
  bright_white: '\x1b[97m',
  bold_red: '\x1b[1;31m',
  bold_green: '\x1b[1;32m',
  bold_yellow: '\x1b[1;33m',
  bold_blue: '\x1b[1;34m',
  bold_magenta: '\x1b[1;35m',
  bold_cyan: '\x1b[1;36m',
  bold_white: '\x1b[1;37m',
};

function colorize(template: string): string {
  let out = template.replace(/\{fg:(\d+)\}/g, (_, n) => `\x1b[38;5;${n}m`);
  out = out.replace(/\{bg:(\d+)\}/g, (_, n) => `\x1b[48;5;${n}m`);
  return out.replace(/\{([a-z_]+)\}/g, (m, name) => COLOR_TOKENS[name] ?? m);
}

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
      highlight('cure.warm_fuzzy', 'You feel warm and fuzzy\\.$', GREEN),
      highlight('cure.righteous', 'You feel righteous\\.$', GREEN),
      highlight('cure.less_sick', 'You feel less sick\\.$', GREEN),
      highlight('cure.no_longer_poisoned', 'You are no longer poisoned\\.$', GREEN),
      highlight('cure.less_tired', 'You feel less tired\\.$', GREEN),
    ],
  },

  // ── Defensive Combat ──────────────────────────────────────────────
  {
    id: 'defensive_combat',
    category: 'defensive',
    name: 'Parries, dodges, blocks',
    description:
      'Tints your defensive saves green so you can read combat at a ' +
      'glance instead of squinting at every line.',
    defaultEnabled: true,
    triggers: [
      highlight('def.parry', '^You parry .+ attack', SOFT_GREEN),
      highlight('def.dodge', '^You dodge .+\\.$', SOFT_GREEN),
      highlight('def.block_shield', '^You block .+ with your shield\\.$', SOFT_GREEN),
      highlight('def.block_weapon', '^You block .+ attack with your weapon\\.$', SOFT_GREEN),
      highlight('def.dual_parry', '^You dual parry .+ attack\\.$', SOFT_GREEN),
      highlight('def.reverse', '^You reverse .+ attack', SOFT_GREEN),
      highlight('def.swing_through', '^You swing right through .+ blurred image', SOFT_GREEN),
      highlight('def.misses', '^.+ swings wildly and misses you by a mile\\.$', SOFT_GREEN),
      highlight('def.shadows_envelop', '^Shadows envelop .+\\.$', SOFT_GREEN),
      highlight('def.terra_shield', '^Your Terra shield deflects the attack\\.$', SOFT_GREEN),
    ],
  },

  // ── Your Buffs Fading ─────────────────────────────────────────────
  // Patterns lifted verbatim from ~/tintin/highlights.tin lines
  // 130-134 (the SUBSTITUTE block), so these match Aabahran's actual
  // wording. Each one prefixes the line with a `##` banner like
  // Magick / TinTin do, plus dim red color for the rest.
  {
    id: 'buff_falls_self',
    category: 'buff_falls_self',
    name: 'Your sanc / bless / haste etc. dropping',
    description:
      'Magick-style `## ` banner + dim red on YOUR buffs falling. Pulled ' +
      'from your highlights.tin so the patterns match Aabahran exactly.',
    defaultEnabled: true,
    triggers: [
      // Sanctuary on yourself.
      replace(
        'buff_self.sanc',
        '^The white aura around your body fades\\.$',
        '{bold_red}## {red}$0{reset}',
        10,
      ),
      // Protection.
      replace(
        'buff_self.protection',
        '^The protective aura around your body fades\\.$',
        '{bold_red}## {red}$0{reset}',
      ),
      // Stone skin (shards of metal).
      replace(
        'buff_self.stoneskin',
        '^The shards of metal protecting you fall to the ground\\.$',
        '{bold_red}## {red}$0{reset}',
      ),
      // Spell turning.
      replace(
        'buff_self.spell_turning',
        '^Your shield of spell turning collapses\\.$',
        '{bold_red}## {red}$0{reset}',
      ),
    ],
  },

  // ── Others' Buffs Fading ──────────────────────────────────────────
  // The same patterns but with %1 / capture-group wildcards so other
  // players' buffs trigger too. Off by default; in groups this fires
  // every couple ticks and gets noisy fast.
  {
    id: 'buff_falls_others',
    category: 'buff_falls_others',
    name: "Others' buffs fading",
    description:
      "Same `## ` banner for OTHER characters' buffs (sanc, protection, " +
      'stoneskin, shield). Off by default since group play floods this.',
    defaultEnabled: false,
    triggers: [
      // Sanctuary on anyone — captures whatever sits between
      // "around" and "fades". Runs at lower priority than the
      // self-only sanc trigger above; once the self trigger replaces
      // its match with "## ..." (ANSI prefix), this pattern's `^`
      // anchor no longer matches the modified line, so the self
      // version wins for "your body" and this one only fires on
      // others.
      replace(
        'buff_other.sanc',
        '^The white aura around (.+) fades\\.$',
        '{bold_yellow}## {yellow}$0{reset}',
        4,
      ),
      // Generic protective shield drop on another character.
      replace(
        'buff_other.shield',
        '^([A-Z][a-z]+) protective shield dissipates\\.$',
        '{bold_yellow}## {yellow}$0{reset}',
        4,
      ),
    ],
  },

  // ── Debuffs wearing off ───────────────────────────────────────────
  // (debuff_falls removed: every pattern was a guess from generic ROM
  // 2.4 conventions, not lifted from your highlights.tin. Add back
  // once we have actual Aabahran wording captured.)

  // (recall_events removed: all 3 patterns were guessed from
  // generic ROM 2.4 wording, not lifted from your highlights.tin.
  // Capture actual Aabahran recall messages in-game to restore.)

  // ── Combat & Spell Events ─────────────────────────────────────────
  // Disarm patterns from highlights.tin lines 105-106 — they tell you
  // exactly which weapon (primary vs secondary) flew off.
  {
    id: 'disarm_events',
    category: 'events',
    name: 'You got disarmed',
    description:
      "Calls out which weapon (primary vs secondary) you just lost. " +
      'Pulled from your highlights.tin so the match is exact.',
    defaultEnabled: true,
    triggers: [
      replace(
        'disarm.secondary',
        '^([A-Z][a-z]+) disarms you and sends your secondary weapon flying!$',
        '{bold_red}## {yellow}$1 disarms you and sends your SECONDARY weapon flying!{reset}',
      ),
      replace(
        'disarm.primary',
        '^([A-Z][a-z]+) disarms you and sends your weapon flying!$',
        '{bold_red}## {yellow}$1 disarms you and sends your PRIMARY weapon flying!{reset}',
      ),
    ],
  },
  {
    id: 'terror_events',
    category: 'events',
    name: 'Fear / terror weapon drops',
    description:
      'Highlights when fear makes you drop your weapon. From ' +
      'highlights.tin line 124.',
    defaultEnabled: true,
    triggers: [
      highlight(
        'terror.drop',
        '^Filled with terror, your weapon slips through your slippery fingers\\.$',
        RED,
      ),
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
