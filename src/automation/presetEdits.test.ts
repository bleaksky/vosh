import { describe, expect, it } from 'vitest';
import { colorize } from './colorTokens';
import {
  alsoKey,
  applyRows,
  buildPreset,
  cardEdits,
  cardFlags,
  changedRows,
  diff,
  editColors,
  editsToSave,
  fixedColorHex,
  flagCount,
  flaggedColors,
  flaggedRows,
  hold,
  keepMine,
  NO_ROW,
  overlay,
  patternKey,
  presetCard,
  shippedCard,
  triggerRows,
  withColorEdit,
  withColorKept,
  withRow,
} from './presetEdits';
import { withReplaceTemplate } from './automationTriggers';
import { drawSample } from './presetSample';
import { presetById, presetTriggers, PRESETS, type Preset, type PresetTrigger } from './presets';
import type { EditRow, PresetEdit } from '../ipc/presetEdits';

const DUAL = 'get 1.;dual 1.';
const WIELD = 'get 1.;wield 1.';
const SECONDARY = 'disarm.secondary';

const disarms = presetById('disarm_buff_fade')!;

/** Disarms and fading buffs with disarm.secondary sending `send`, as it
 *  sent before the fix of October 5 with WIELD. */
function withSecondarySend(send: string): Preset {
  return {
    ...disarms,
    triggers: disarms.triggers.map((t) =>
      t.name === SECONDARY
        ? {
            ...t,
            actions: t.actions.map((a) => (a.kind === 'send' ? { ...a, template: send } : a)),
          }
        : t,
    ),
  };
}

function trigger(preset: Preset, name: string): PresetTrigger {
  return preset.triggers.find((t) => t.name === name)!;
}

function sendOf(build: ReturnType<typeof buildPreset>, name: string): string | undefined {
  const t = build.triggers.find((x) => x.name === name)!;
  const send = t.actions.find((a) => a.kind === 'send');
  return send?.kind === 'send' ? send.template : undefined;
}

const secondaryEdit = (row: EditRow): PresetEdit => ({ triggers: { [SECONDARY]: { send: row } } });

const ref = (row: string | null, trigger: string | null = SECONDARY) => ({
  preset: 'disarm_buff_fade',
  trigger,
  row,
});

describe('buildPreset with no edits', () => {
  it('builds every preset byte for byte as it ships', () => {
    for (const preset of PRESETS) {
      const build = buildPreset(preset);
      expect(JSON.stringify(build.triggers)).toBe(JSON.stringify(presetTriggers(preset)));
      expect(build).toMatchObject({ write: {}, told: [], removed: [] });
    }
  });

  it('leaves the group out, so Rust keeps the stored copy', () => {
    for (const t of buildPreset(disarms).triggers) expect('group' in t).toBe(false);
  });
});

// Board 4's table, every edit of Then send on disarm.secondary against
// the dual fix.
describe('the dual fix on disarm.secondary', () => {
  it('lands where you made no edit', () => {
    const build = buildPreset(disarms);
    expect(sendOf(build, SECONDARY)).toBe(DUAL);
  });

  it('folds an edit that now equals the fix, with no notice', () => {
    const build = buildPreset(disarms, secondaryEdit({ value: DUAL, was: WIELD }));
    expect(sendOf(build, SECONDARY)).toBe(DUAL);
    expect(build.told).toEqual([]);
    // Its value is its was, so Rust drops the row and the pencil goes.
    expect(build.write).toEqual({
      triggers: { [SECONDARY]: { send: { value: WIELD, was: WIELD } } },
    });
  });

  it('keeps get 1. and flags it, telling the fix once', () => {
    const build = buildPreset(disarms, secondaryEdit({ value: 'get 1.', was: WIELD }));
    expect(sendOf(build, SECONDARY)).toBe('get 1.');
    expect(build.told).toEqual([ref('send')]);
    expect(build.write).toEqual({
      triggers: { [SECONDARY]: { send: { value: 'get 1.', was: DUAL, seen: DUAL } } },
    });
  });

  it('keeps a cleared send and flags it', () => {
    const build = buildPreset(disarms, secondaryEdit({ value: '', was: WIELD }));
    expect(sendOf(build, SECONDARY)).toBeUndefined();
    expect(build.told).toEqual([ref('send')]);
  });

  it('stays flagged and quiet once the notice named the fix', () => {
    const seen = secondaryEdit({ value: '', was: WIELD, seen: DUAL });
    const build = buildPreset(disarms, seen);
    expect(sendOf(build, SECONDARY)).toBeUndefined();
    expect(build.told).toEqual([]);
    expect(build.write).toEqual({});
    expect(overlay(trigger(disarms, SECONDARY), seen.triggers![SECONDARY]).holds).toEqual({
      send: 'flagged',
    });
  });

  it('tells a second fix to the same row again', () => {
    const second = withSecondarySend('get 2.;dual 2.');
    const build = buildPreset(second, secondaryEdit({ value: '', was: WIELD, seen: DUAL }));
    expect(build.told).toEqual([ref('send')]);
    expect(build.write.triggers![SECONDARY].send.seen).toBe('get 2.;dual 2.');
  });

  it('holds before the fix as an edit that applies', () => {
    const before = withSecondarySend(WIELD);
    const build = buildPreset(before, secondaryEdit({ value: 'get 1.', was: WIELD }));
    expect(sendOf(build, SECONDARY)).toBe('get 1.');
    expect(build).toMatchObject({ write: {}, told: [], removed: [] });
  });
});

describe('a swatch', () => {
  const LILAC = '#c3a6ff';
  const lilac = (was: string, seen?: string): PresetEdit => ({
    colors: { line: seen ? { value: LILAC, was, seen } : { value: LILAC, was } },
  });
  const moved: Preset = {
    ...disarms,
    colors: { ...disarms.colors, line: { ...disarms.colors.line, token: 'fg:179' } },
  };

  it('paints every trigger that uses its key', () => {
    const build = buildPreset(disarms, lilac('fg:178'));
    expect(build.triggers).toEqual(presetTriggers(disarms, { line: LILAC }));
    expect(build).toMatchObject({ write: {}, told: [], removed: [] });
  });

  it('follows the same rule when a fix changes its color', () => {
    const build = buildPreset(moved, lilac('fg:178'));
    expect(build.triggers).toEqual(presetTriggers(moved, { line: LILAC }));
    expect(build.told).toEqual([ref('line', null)]);
    expect(build.write).toEqual({
      colors: { line: { value: LILAC, was: 'fg:179', seen: 'fg:179' } },
    });
    expect(buildPreset(moved, lilac('fg:178', 'fg:179')).told).toEqual([]);
  });

  it('folds once the preset paints your color', () => {
    const yours: Preset = {
      ...disarms,
      colors: { ...disarms.colors, line: { ...disarms.colors.line, token: LILAC } },
    };
    const build = buildPreset(yours, lilac('fg:178'));
    expect(build.told).toEqual([]);
    expect(build.write).toEqual({ colors: { line: { value: 'fg:178', was: 'fg:178' } } });
  });

  it('is a removed row once the preset no longer has its key', () => {
    const build = buildPreset(disarms, {
      colors: { glow: { value: LILAC, was: 'fg:200' } },
    });
    expect(build.triggers).toEqual(presetTriggers(disarms));
    expect(build.removed).toEqual([ref('glow', null)]);
  });
});

describe('Replace with keeps the color keys', () => {
  const TURNING = 'buff.spell_turning';
  const shipped = '{mark}##{reset} {line}Your shield of spell turning collapses.{reset}';
  const unmarked = '{line}Your shield of spell turning collapses.{reset}';
  const edit = (colors: PresetEdit['colors'] = {}): PresetEdit => ({
    colors,
    triggers: { [TURNING]: { replace: { value: unmarked, was: shipped } } },
  });

  it('reads the preset template with its keys', () => {
    expect(triggerRows(trigger(disarms, TURNING)).replace).toBe(shipped);
  });

  it('never reads a color change as a fix', () => {
    const build = buildPreset(disarms, edit({ line: { value: '#c3a6ff', was: 'fg:178' } }));
    const t = build.triggers.find((x) => x.name === TURNING)!;
    expect(t.actions).toEqual([
      {
        kind: 'replace',
        template: colorize('{#c3a6ff}Your shield of spell turning collapses.{reset}'),
      },
    ]);
    expect(build).toMatchObject({ write: {}, told: [], removed: [] });
  });

  it('never reads a color reset as a fix', () => {
    const build = buildPreset(disarms, edit());
    const t = build.triggers.find((x) => x.name === TURNING)!;
    expect(t.actions).toEqual([
      {
        kind: 'replace',
        template: colorize('{fg:178}Your shield of spell turning collapses.{reset}'),
      },
    ]);
    expect(build.told).toEqual([]);
  });
});

describe('a trigger the preset no longer builds', () => {
  const gone: PresetEdit = {
    triggers: { 'disarm.tertiary': { enabled: { value: false, was: true } } },
  };

  it('is named once and its edits come out', () => {
    const build = buildPreset(disarms, gone);
    expect(build.removed).toEqual([ref(null, 'disarm.tertiary')]);
    expect(build.triggers.map((t) => t.name)).not.toContain('disarm.tertiary');
    // Every row Rust drops, so the next launch finds nothing to name.
    expect(build.write).toEqual({
      triggers: { 'disarm.tertiary': { enabled: { value: true, was: true } } },
    });
  });
});

describe('rows in a list', () => {
  const room = presetById('room_and_time')!;
  const weather = trigger(room, 'weather.change');
  const first = weather.patterns[0].pattern;
  const off: Record<string, EditRow> = {
    [patternKey(first)]: {
      value: { text: first, enabled: false },
      was: { text: first, enabled: true },
    },
  };

  it('key a pattern by its text, so an edit stays on its row when a fix adds one', () => {
    const added: PresetTrigger = {
      ...weather,
      patterns: [{ pattern: '^The fog thickens\\.$', enabled: true }, ...weather.patterns],
    };
    const laid = overlay(added, off);
    expect(laid.holds).toEqual({ [patternKey(first)]: 'applies' });
    expect(laid.trigger.patterns[0]).toEqual({ pattern: '^The fog thickens\\.$', enabled: true });
    expect(laid.trigger.patterns[1]).toEqual({ pattern: first, enabled: false });
  });

  it('count a pattern the preset rewrote as a removed row', () => {
    const rewritten: PresetTrigger = {
      ...weather,
      patterns: [
        { pattern: '^The sky clouds over\\.$', enabled: true },
        ...weather.patterns.slice(1),
      ],
    };
    expect(overlay(rewritten, off).holds).toEqual({ [patternKey(first)]: 'removed' });
  });

  it('take a pattern or an Also send you added', () => {
    const laid = overlay(weather, {
      [patternKey('^The fog thickens\\.$')]: {
        value: { text: '^The fog thickens\\.$', enabled: true },
        was: NO_ROW,
      },
      [alsoKey('look')]: { value: 'look', was: NO_ROW },
    });
    expect(laid.trigger.patterns.at(-1)).toEqual({
      pattern: '^The fog thickens\\.$',
      enabled: true,
    });
    expect(laid.trigger.actions.at(-1)).toEqual({ kind: 'send', template: 'look' });
  });

  it('take out a row you removed', () => {
    const laid = overlay(weather, {
      [patternKey(first)]: { value: NO_ROW, was: { text: first, enabled: true } },
    });
    expect(laid.trigger.patterns).toEqual(weather.patterns.slice(1));
  });
});

describe('applyRows', () => {
  it('lays a group, a priority, Match and a color of your own over a preset trigger', () => {
    const cure = trigger(presetById('healing_basics')!, 'cure.less_sick');
    const t = applyRows(cure, { group: 'Healing', priority: 7, target: 'prompt', fg: 'red' });
    expect(t).toMatchObject({ group: 'Healing', priority: 7, target: 'prompt' });
    expect(t.actions).toEqual([{ kind: 'highlight', style: { fg: 'red' } }]);
    expect(triggerRows(cure).fg).toBe('{line}');
  });

  it('turns Then send off with an empty row and on with a command', () => {
    const primary = trigger(disarms, 'disarm.primary');
    expect(applyRows(primary, { send: NO_ROW }).actions.map((a) => a.kind)).toEqual(['replace']);
    expect(applyRows(primary, { send: 'get all' }).actions[1]).toEqual({
      kind: 'send',
      template: 'get all',
    });
  });

  it('reads every pattern in the mode you pick', () => {
    const t = applyRows(trigger(disarms, 'disarm.primary'), { mode: 'starts_with' });
    expect(t.patterns[0].mode).toBe('starts_with');
  });
});

describe('hold', () => {
  it('reads a row the preset no longer has as removed', () => {
    expect(hold('stance', { value: 'x', was: 'y' }, undefined)).toBe('removed');
  });

  it('flags a field the fix emptied, since a field is no list', () => {
    expect(hold('send', { value: 'get 1.', was: WIELD }, NO_ROW)).toBe('flagged');
  });
});

describe('diff', () => {
  const primary = trigger(disarms, 'disarm.primary');
  const now = triggerRows(primary);

  it('gives only the rows that changed on the page, each against the preset now', () => {
    const left = { ...now, priority: 7, enabled: false };
    expect(diff(now, left, now)).toEqual({
      priority: { value: 7, was: 5 },
      enabled: { value: false, was: true },
    });
    expect(diff(now, now, now)).toEqual({});
  });

  it('keeps the seen of a flagged row you change again', () => {
    const held: Record<string, EditRow> = { send: { value: '', was: WIELD, seen: DUAL } };
    const loaded = { ...now, send: '' };
    expect(diff(loaded, { ...loaded, send: 'get 1.' }, now, held)).toEqual({
      send: { value: 'get 1.', was: WIELD, seen: DUAL },
    });
  });
});

describe('keepMine', () => {
  const flagged: EditRow = { value: '', was: WIELD, seen: DUAL };

  it('keep mine by moving was and clearing seen', () => {
    const kept = keepMine(flagged, DUAL);
    expect(kept).toEqual({ value: '', was: DUAL });
    expect(hold('send', kept, DUAL)).toBe('applies');
  });
});

// The swatches of a preset's card (Presets board 1, Q4 and Q8).
describe('the swatches of the card', () => {
  const lilac: PresetEdit = { colors: { line: { value: '#c3a6ff', was: 'fg:178' } } };

  it('names the hex of a fixed or true color and none for a theme color', () => {
    expect(fixedColorHex('fg:178')).toBe('#d7af00');
    expect(fixedColorHex('fg:244')).toBe('#808080');
    expect(fixedColorHex('#8FA7D9')).toBe('#8fa7d9');
    expect(fixedColorHex('bold_red')).toBeNull();
    expect(fixedColorHex('green')).toBeNull();
  });

  it('sets a swatch from the preset color, and its own token or a clear is no edit', () => {
    expect(withColorEdit(disarms, undefined, 'line', '#c3a6ff')).toEqual(lilac);
    expect(withColorEdit(disarms, lilac, 'line', null)).toBeUndefined();
    expect(withColorEdit(disarms, lilac, 'line', 'fg:178')).toBeUndefined();
    // A row you change again keeps the value you first changed it from.
    const flagged: PresetEdit = { colors: { line: { value: '#c3a6ff', was: 'fg:172' } } };
    expect(withColorEdit(disarms, flagged, 'line', '#ffffff')).toEqual({
      colors: { line: { value: '#ffffff', was: 'fg:172' } },
    });
  });

  it('sends only what changed, and a row that went as the preset value', () => {
    expect(editsToSave(disarms, lilac, lilac)).toBeNull();
    expect(editsToSave(disarms, undefined, lilac)).toEqual(lilac);
    expect(editsToSave(disarms, lilac, undefined)).toEqual({
      colors: { line: { value: 'fg:178', was: 'fg:178' } },
    });
    const sanctuary: PresetEdit = {
      ...lilac,
      triggers: { 'buff.sanctuary': { enabled: { value: false, was: true } } },
    };
    expect(editsToSave(disarms, sanctuary, undefined)).toEqual({
      colors: { line: { value: 'fg:178', was: 'fg:178' } },
      triggers: { 'buff.sanctuary': { enabled: { value: true, was: true } } },
    });
  });

  // Board 4: a fix that changes a swatch you changed.
  it('flags a swatch a fix changed, and Keep mine or Take the fix clears it', () => {
    const flagged: PresetEdit = {
      colors: { line: { value: '#c3a6ff', was: 'fg:172', seen: 'fg:178' } },
    };
    expect(flaggedColors(disarms, flagged)).toEqual({ line: 'fg:178' });
    expect(flaggedColors(disarms, lilac)).toEqual({});
    const kept = withColorKept(disarms, flagged, 'line');
    expect(kept).toEqual({ colors: { line: { value: '#c3a6ff', was: 'fg:178' } } });
    expect(flaggedColors(disarms, kept)).toEqual({});
    expect(editsToSave(disarms, flagged, kept)).toEqual(kept);
    // Take the fix clears the swatch, which Rust drops.
    expect(editsToSave(disarms, flagged, withColorEdit(disarms, flagged, 'line', null))).toEqual({
      colors: { line: { value: 'fg:178', was: 'fg:178' } },
    });
    // A new color keeps the flag until you choose.
    expect(withColorEdit(disarms, flagged, 'line', '#ffffff')).toEqual({
      colors: { line: { value: '#ffffff', was: 'fg:172', seen: 'fg:178' } },
    });
  });

  it('folds a hex that is the preset color', () => {
    const gold: PresetEdit = { colors: { line: { value: '#d7af00', was: 'fg:178' } } };
    expect(editsToSave(disarms, undefined, gold)).toEqual({
      colors: { line: { value: 'fg:178', was: 'fg:178' } },
    });
  });

  it('draws Looks like in your colors', () => {
    const runs = drawSample(disarms, disarms.sample[0], editColors(lilac)).runs;
    expect(runs.map(([, color]) => color)).toEqual(['bold red', null, '#c3a6ff']);
    expect(drawSample(disarms, disarms.sample[0]).runs.at(-1)?.[1]).toBe('178');
  });
});

// Presets board 2: a preset trigger's card in Triggers, and what its
// Save sends.
describe('the card of a preset trigger', () => {
  const SANCTUARY = 'buff.sanctuary';
  const stored = (name: string, edit?: PresetEdit) =>
    buildPreset(disarms, edit).triggers.find((t) => t.name === name)!;
  const off: PresetEdit = { triggers: { [SANCTUARY]: { enabled: { value: false, was: true } } } };

  it('shows the color keys, your rows and the group the store keeps', () => {
    const card = presetCard({ ...stored(SANCTUARY, off), group: 'fights' }, off)!;
    expect(card.enabled).toBe(false);
    expect(card.group).toBe('fights');
    expect(card.preset).toBe('disarm_buff_fade');
    expect(triggerRows(card as unknown as PresetTrigger).replace).toBe(
      '{mark}##{reset} {line}The protective aura around $1 fades.{reset}',
    );
    expect(changedRows(card)).toEqual({ enabled: true, group: NO_ROW });
  });

  it('is undefined for a trigger the library does not build', () => {
    expect(presetCard({ ...stored(SANCTUARY), name: 'buff.gone' }, undefined)).toBeUndefined();
    expect(presetCard({ ...stored(SANCTUARY), preset: 'gone' }, undefined)).toBeUndefined();
  });

  it('goes back to the preset whole, its group included', () => {
    const card = presetCard({ ...stored(SANCTUARY, off), group: 'fights' }, off)!;
    const reset = shippedCard(card);
    expect(changedRows(reset)).toEqual({});
    expect('group' in reset).toBe(false);
  });

  it('sends only the rows you changed, each from the preset', () => {
    const before = presetCard(stored(SANCTUARY), undefined)!;
    const after = { ...before, enabled: false, priority: 7 };
    expect(cardEdits([{ before, after }], {})).toEqual(
      new Map([
        [
          'disarm_buff_fade',
          {
            triggers: {
              [SANCTUARY]: {
                enabled: { value: false, was: true },
                priority: { value: 7, was: 5 },
              },
            },
          },
        ],
      ]),
    );
  });

  it('folds a row set back to the preset at Save', () => {
    const before = presetCard(stored(SANCTUARY, off), off)!;
    const after = { ...before, enabled: true };
    const sent = cardEdits([{ before, after }], { disarm_buff_fade: off });
    // A row whose value is its was, which preset_edits_set drops.
    expect(sent.get('disarm_buff_fade')).toEqual({
      triggers: { [SANCTUARY]: { enabled: { value: true, was: true } } },
    });
  });

  it('leaves out a flagged row you never touched', () => {
    const flagged: PresetEdit = {
      triggers: { [SECONDARY]: { send: { value: '', was: DUAL, seen: DUAL } } },
    };
    const before = presetCard(stored(SECONDARY, flagged), flagged)!;
    const after = { ...before, priority: 7 };
    expect(cardEdits([{ before, after }], { disarm_buff_fade: flagged })).toEqual(
      new Map([
        ['disarm_buff_fade', { triggers: { [SECONDARY]: { priority: { value: 7, was: 5 } } } }],
      ]),
    );
  });

  it('keeps the keys of a Replace with edit, so a later swatch reaches it', () => {
    const TURNING = 'buff.spell_turning';
    const before = presetCard(stored(TURNING), undefined)!;
    const unmarked = '{line}Your shield of spell turning collapses.{reset}';
    const after = { ...before, actions: withReplaceTemplate(before.actions, unmarked) };
    const edit = cardEdits([{ before, after }], {}).get('disarm_buff_fade')!;
    expect(edit.triggers![TURNING].replace.value).toBe(unmarked);

    const lilac = { line: { value: '#c3a6ff', was: 'fg:178' } };
    const t = buildPreset(disarms, { ...edit, colors: lilac }).triggers.find(
      (x) => x.name === TURNING,
    )!;
    expect(t.actions).toEqual([
      {
        kind: 'replace',
        template: colorize('{#c3a6ff}Your shield of spell turning collapses.{reset}'),
      },
    ]);
  });

  // Board 4: Orla cleared Then send before the dual fix.
  const cleared: PresetEdit = {
    triggers: { [SECONDARY]: { send: { value: '', was: WIELD, seen: DUAL } } },
  };

  it('flags the row a fix changed under your edit, with the preset value now', () => {
    expect(flaggedRows(trigger(disarms, SECONDARY), cleared.triggers![SECONDARY])).toEqual({
      send: DUAL,
    });
    expect(flagCount(disarms, cleared)).toBe(1);
    expect(flagCount(disarms, undefined)).toBe(0);
    const card = presetCard(stored(SECONDARY, cleared), cleared)!;
    expect(cardFlags(card, cleared.triggers![SECONDARY])).toEqual({ send: DUAL });
    expect(cardFlags(card, undefined)).toEqual({});
  });

  it('drops the flag once the page takes the fix or keeps yours', () => {
    const held = cleared.triggers![SECONDARY];
    const card = presetCard(stored(SECONDARY, cleared), cleared)!;
    expect(cardFlags(withRow(card, 'send', DUAL), held)).toEqual({});
    expect(cardFlags({ ...card, kept: ['send'] }, held)).toEqual({});
  });

  it('sends Keep mine with the preset value now as its was and no seen', () => {
    const before = presetCard(stored(SECONDARY, cleared), cleared)!;
    const sent = cardEdits([{ before, after: { ...before, kept: ['send'] } }], {
      disarm_buff_fade: cleared,
    });
    expect(sent.get('disarm_buff_fade')).toEqual({
      triggers: { [SECONDARY]: { send: { value: '', was: DUAL } } },
    });
    // Keep mine and a new value of your own send that value.
    const typed = { ...withRow(before, 'send', 'get 1.'), kept: ['send'] };
    expect(
      cardEdits([{ before, after: typed }], { disarm_buff_fade: cleared }).get('disarm_buff_fade'),
    ).toEqual({ triggers: { [SECONDARY]: { send: { value: 'get 1.', was: DUAL } } } });
  });

  it('sends Take the fix as a row Rust drops', () => {
    const before = presetCard(stored(SECONDARY, cleared), cleared)!;
    const after = withRow(before, 'send', DUAL);
    expect(
      cardEdits([{ before, after }], { disarm_buff_fade: cleared }).get('disarm_buff_fade'),
    ).toEqual({ triggers: { [SECONDARY]: { send: { value: DUAL, was: DUAL, seen: DUAL } } } });
  });

  it('skips a trigger the library does not build', () => {
    const before = { ...stored(SANCTUARY), name: 'buff.gone' };
    expect(cardEdits([{ before, after: { ...before, enabled: false } }], {}).size).toBe(0);
  });
});
