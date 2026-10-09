import { describe, expect, it } from 'vitest';
import {
  fieldName,
  flatRows,
  LAYOUT_TOKENS,
  needsCode,
  paramPrompt,
  pickerGroups,
  rowKey,
  rowStatus,
  sourceLine,
} from './pickerRows';
import type { PromptFieldState } from '../ipc/prompt';

/** A catalog field as prompt_state_get reports it. */
function field(name: string, over: Partial<PromptFieldState> = {}): PromptFieldState {
  return {
    name,
    label: name,
    aliases: [],
    kind: 'num',
    group: 'vitals',
    package: null,
    new_build: false,
    codes: [],
    search: [],
    param: false,
    listed: true,
    state: 'missing',
    source: null,
    value: null,
    max: null,
    sent: true,
    in_prompt: false,
    ...over,
  };
}

// Ilsabet out of a fight on the new build, his PROMPT with
// no %S or %b, Char.State and Room.Info sent.
const HEALTH = field('hp', {
  label: 'Health',
  kind: 'gauge',
  codes: ['%h', '%H', '%K'],
  package: 'Char.Vitals',
  state: 'value',
  value: '1020',
  max: '1020',
  in_prompt: true,
  search: ['hp'],
});
const OPPONENT = field('opponent', {
  label: 'Opponent',
  group: 'fight',
  package: 'Char.Combat',
  state: 'absent',
});
const POSITION = field('pos', {
  label: 'Position',
  group: 'fight',
  codes: ['%S'],
  package: 'Char.State',
  new_build: true,
  state: 'value',
  value: 'standing',
});
const AREA_NUM = field('area_num', { label: 'Area number', group: 'room', codes: ['%b'] });
const EXITS = field('exits', {
  label: 'Exits',
  group: 'room',
  codes: ['%e'],
  package: 'Room.Info',
  new_build: true,
  state: 'value',
  value: 'S',
});
const TNL = field('tnl', {
  label: 'To next level',
  group: 'worth',
  codes: ['%X'],
  package: 'Char.Worth',
  search: ['tnl'],
  state: 'value',
  value: '2345',
});
const EXP = field('exp', {
  label: 'Experience',
  group: 'worth',
  codes: ['%x'],
  package: 'Char.Worth',
  search: ['xp'],
  state: 'value',
  value: '123456',
});
const AFF = field('aff', {
  label: 'An affect…',
  group: 'affects',
  param: true,
  package: 'Char.Affects',
});
const OLC = field('olc', { label: 'OLC editor', group: 'building', codes: ['%o'] });
const GMCP = field('gmcp', { label: 'More from the game', group: 'more', param: true });
const MAXHP = field('maxhp', { listed: false });

const FIGHT = field('fight', { label: 'In a fight', group: 'fight', kind: 'flag' });

const CATALOG = [
  HEALTH,
  MAXHP,
  FIGHT,
  OPPONENT,
  POSITION,
  AREA_NUM,
  EXITS,
  TNL,
  EXP,
  AFF,
  OLC,
  GMCP,
];

describe('what each row reads', () => {
  it('reads the value, why a fight value is empty, or which code it needs', () => {
    expect(rowStatus(HEALTH)).toBe('1020');
    expect(rowStatus(OPPONENT)).toBe('in a fight');
    expect(rowStatus(POSITION)).toBe('standing');
    expect(rowStatus(AREA_NUM)).toBe('not in your prompt');
    expect(rowStatus(EXITS)).toBe('S');
    expect(rowStatus({ ...HEALTH, state: 'hidden', value: null })).toBe('hidden');
    expect(rowStatus({ ...TNL, state: 'missing', value: null, sent: false, in_prompt: true })).toBe(
      'not sent yet',
    );
  });

  it('reads a new build package as prompt only until it comes (D26)', () => {
    const older = { ...EXITS, state: 'missing' as const, value: null, sent: false };
    expect(needsCode(older)).toBe(true);
    expect(rowStatus(older)).toBe('not in your prompt');
    expect(sourceLine(older)).toBe('Add %e to your prompt in the game to use it.');
    expect(needsCode(EXITS)).toBe(false);
  });

  it('says where a field comes from', () => {
    expect(sourceLine(HEALTH)).toBe(
      'From your prompt, and from the game when your prompt leaves it out.',
    );
    expect(sourceLine(EXITS)).toBe(
      'From your prompt, and from the game when your prompt leaves it out.',
    );
    expect(sourceLine({ ...AREA_NUM, in_prompt: true })).toBe(
      'From your prompt only. The game sends it nowhere else.',
    );
    expect(sourceLine(AREA_NUM)).toBe('Add %b to your prompt in the game to use it.');
    expect(sourceLine(OPPONENT)).toBe('From the game.');
    expect(sourceLine(field('tick', { group: 'vosh' }))).toBe('From Vosh.');
    // The game's prompt is the game's own line, which Vosh keeps.
    expect(sourceLine(field('raw', { group: 'vosh', kind: 'raw' }))).toBe(
      'From the game. Vosh keeps your last prompt as it came.',
    );
    expect(sourceLine(field('mine', { group: 'scripts' }))).toBe('From your scripts.');
  });
});

describe('the topics', () => {
  it('lists the topics in order, Text and layout before More from the game', () => {
    const groups = pickerGroups(CATALOG, [], '');
    expect(groups.map((g) => g.label)).toEqual([
      'Vitals',
      'Fight',
      'Worth',
      'Affects',
      'Room',
      'Text and layout',
      'More from the game',
    ]);
    expect(groups[0].rows.map(rowKey)).toEqual(['field:hp']);
    // In a fight is what When writes, so the picker leaves it out.
    expect(groups[1].rows.map(rowKey)).toEqual(['field:opponent', 'field:pos']);
    expect(groups[5].rows.map(rowKey)).toEqual([
      'layout:nl',
      'layout:nl_fight',
      'layout:space',
      'layout:right',
    ]);
    // Area number draws dim.
    const room = groups.find((g) => g.label === 'Room');
    expect(room?.rows.map((r) => r.dim)).toEqual([true, false]);
  });

  it('shows Building to immortals', () => {
    expect(pickerGroups(CATALOG, ['Imm.Queues'], '').map((g) => g.label)).toContain('Building');
    const shown = { ...OLC, in_prompt: true };
    expect(pickerGroups([shown], [], '').map((g) => g.label)).toContain('Building');
  });

  it('finds a field by its name, another word, or its code as typed', () => {
    const find = (q: string) => flatRows(pickerGroups(CATALOG, [], q)).map(rowKey);
    expect(find('%X')).toEqual(['field:tnl']);
    expect(find('%x')).toEqual(['field:exp']);
    expect(find('hp')).toEqual(['field:hp']);
    expect(find('xp')).toEqual(['field:exp']);
    expect(find('line')).toEqual(['layout:nl', 'layout:nl_fight']);
    expect(find('right')).toEqual(['layout:right']);
    // Edit as text takes the code each one writes.
    expect(LAYOUT_TOKENS).toEqual({
      nl: '%nl',
      nl_fight: '%{if:fight}%nl%{end}',
      space: ' ',
      right: '%{right}',
    });
    expect(find('nothing like it')).toEqual([]);
  });
});

describe('the changes of your vitals', () => {
  // Health change and Health this tick, as prompt_state_get reports them
  // after a hit and a tick.
  const change = (name: string, label: string, value: string | null): PromptFieldState =>
    field(name, {
      label,
      kind: 'change',
      source: value === null ? null : 'vosh',
      state: value === null ? 'absent' : 'value',
      value,
      search: ['hp', 'gain', 'loss'],
    });
  const HP_CHANGE = change('hp_change', 'Health change', '-34');
  const HP_TICK = change('hp_tick', 'Health this tick', null);

  it('lists both under Vitals after Health, each with what it reads now', () => {
    const groups = pickerGroups([HEALTH, HP_CHANGE, HP_TICK], [], '');
    expect(groups[0].label).toBe('Vitals');
    expect(groups[0].rows.map(rowKey)).toEqual(['field:hp', 'field:hp_change', 'field:hp_tick']);
    expect(groups[0].rows.map((r) => r.status)).toEqual(['1020', '-34', '']);
    expect(groups[0].rows.every((r) => !r.dim)).toBe(true);
  });

  it('finds them by health, by tick and by gain', () => {
    const find = (q: string) =>
      flatRows(pickerGroups([HEALTH, HP_CHANGE, HP_TICK], [], q)).map(rowKey);
    expect(find('health')).toEqual(['field:hp', 'field:hp_change', 'field:hp_tick']);
    expect(find('this tick')).toEqual(['field:hp_tick']);
    expect(find('gain')).toEqual(['field:hp_change', 'field:hp_tick']);
  });

  it('says Vosh works each one out', () => {
    expect(sourceLine(HP_CHANGE)).toBe('From Vosh. How much it changed since your last prompt.');
    expect(sourceLine(HP_TICK)).toBe(
      'From Vosh. How much it changed over the last tick, kept until the next one.',
    );
  });
});

describe('a field that takes a name', () => {
  it('writes the name you type into its token', () => {
    expect(fieldName(HEALTH, '')).toBe('hp');
    expect(fieldName(AFF, '')).toBeNull();
    expect(fieldName(AFF, ' giant strength ')).toBe('aff:giant_strength');
    expect(fieldName(GMCP, 'Char.Vitals.ep')).toBe('gmcp:Char.Vitals.ep');
    expect(paramPrompt(AFF)).toEqual({ label: 'Affect', placeholder: 'sanctuary' });
    expect(paramPrompt(GMCP).label).toBe('Path');
  });
});
