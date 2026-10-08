import { describe, expect, it } from 'vitest';
import {
  boxHeight,
  cardAnchor,
  clockTime,
  codeReaderStep,
  cardShowState,
  codesSourceLine,
  editedTable,
  entryCopy,
  headerButtons,
  lastSeenLine,
  legendColumns,
  matchLines,
  matchSentences,
  matchTone,
  migratedNote,
  movedBackTable,
  nextCardRequest,
  sampleCut,
  moreItems,
  nameChoices,
  namesFor,
  numberButtons,
  openingStep,
  placeLabels,
  placeNameButtons,
  prefixNote,
  readMarks,
  readRows,
  savedCapture,
  savedForName,
  withCapture,
  withDesign,
  withMoveTakenBack,
  withShow,
  withStart,
  startRows,
  takeBackOnto,
  undoEntry,
  type CardStep,
} from './cardRules';
import { moveBack, type MoveMade } from './promptPieces';
import { promptShowLock } from './showState';
import type {
  PromptCapture,
  PromptCaptureCheck,
  PromptCheckRead,
  PromptConfig,
  PromptLineNumber,
  PromptPreset,
  PromptShowState,
} from '../ipc/prompt';

const none: PromptCapture = { kind: 'none' };
const codes: PromptCapture = {
  kind: 'aabahran',
  prompt: '%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c',
  fprompt: '',
  follow_game: true,
  source: 'gmcp',
};
const migrated: PromptCapture = {
  kind: 'regex',
  lines: ['\\[(?<hp>\\d+)/(?<maxhp>\\d+)hp'],
  settle: false,
  source: 'migrated',
};

describe('where the card opens', () => {
  it('reads the codes the game sent, or asks for them, on The Forsaken Lands', () => {
    expect(openingStep({ capture: none, forsaken: true, gameSent: true })).toBe('codes');
    expect(openingStep({ capture: none, forsaken: true, gameSent: false })).toBe('codes-entry');
  });

  it('points at the line on any other game', () => {
    expect(openingStep({ capture: none, forsaken: false, gameSent: false })).toBe('point');
  });

  it('replaces the pattern your old capture trigger left with the codes', () => {
    expect(openingStep({ capture: migrated, forsaken: true, gameSent: true })).toBe('codes');
    // Elsewhere the pattern is all Vosh has, so the card rests on it.
    expect(openingStep({ capture: migrated, forsaken: false, gameSent: false })).toBe('rest');
  });

  it('reads the codes the game sent once you choose the code reader on another host', () => {
    // A local server of The Forsaken Lands gets its rules from More.
    // The game may have sent your codes already, which the card reads
    // at once, as it does on the game's own host.
    expect(codeReaderStep(true)).toBe('codes');
    expect(codeReaderStep(false)).toBe('codes-entry');
  });

  it('rests once the profile reads its prompt', () => {
    expect(openingStep({ capture: codes, forsaken: true, gameSent: true })).toBe('rest');
    const typed: PromptCapture = { kind: 'regex', lines: ['^> $'], settle: true, source: 'typed' };
    expect(openingStep({ capture: typed, forsaken: true, gameSent: true })).toBe('rest');
  });
});

describe('the header', () => {
  const steps: CardStep[] = ['codes-entry', 'codes', 'point', 'name', 'start', 'rest'];

  it('shows Close alone on the capture steps and Edit as text once a design is there', () => {
    const shown = steps.map((step) => [
      step,
      headerButtons(step, moreItems({ step, forsaken: true, gameSent: true, capture: none })),
    ]);
    expect(shown).toEqual([
      ['codes-entry', { editAsText: false, more: false }],
      ['codes', { editAsText: false, more: false }],
      ['point', { editAsText: false, more: false }],
      ['name', { editAsText: false, more: false }],
      ['start', { editAsText: true, more: true }],
      ['rest', { editAsText: true, more: true }],
    ]);
  });

  it('names the logged in character when it owns the profile, else the profile', () => {
    const identity = {
      host: 'play.theforsakenlands.com',
      port: 1848,
      character: 'Tester',
      profile: 'default',
      claimed_by: 'default',
    };
    expect(savedForName(identity, 'default')).toBe('Saved for Tester');
    expect(savedForName({ ...identity, claimed_by: 'Healer' }, 'default')).toBe(
      'Saved for Default',
    );
    expect(savedForName({ ...identity, character: null }, 'Healer')).toBe('Saved for Healer');
    expect(savedForName(null, 'default')).toBe('Saved for Default');
  });
});

describe('the More menu', () => {
  const ids = (items: ReturnType<typeof moreItems>) =>
    items.map((item) => (item === 'separator' ? '-' : item.id));

  it('offers Point at the line instead and Forget on The Forsaken Lands', () => {
    expect(
      ids(moreItems({ step: 'rest', forsaken: true, gameSent: true, capture: codes })),
    ).toEqual(['point', '-', 'forget']);
  });

  it('offers Change codes first only when the game sent no prompt setting', () => {
    expect(
      ids(moreItems({ step: 'rest', forsaken: true, gameSent: false, capture: codes })),
    ).toEqual(['change-codes', 'point', '-', 'forget']);
  });

  it('offers the code reader on another game, and Forget once there is a capture', () => {
    expect(
      ids(moreItems({ step: 'point', forsaken: false, gameSent: false, capture: none })),
    ).toEqual(['use-codes']);
    expect(
      ids(moreItems({ step: 'rest', forsaken: false, gameSent: false, capture: migrated })),
    ).toEqual(['use-codes', '-', 'forget']);
  });

  it('keeps the labels the boards draw', () => {
    const items = moreItems({ step: 'rest', forsaken: true, gameSent: false, capture: codes });
    expect(items.map((item) => (item === 'separator' ? '-' : item.label))).toEqual([
      'Change codes…',
      'Point at the line instead…',
      '-',
      "Forget your game's prompt",
    ]);
    const other = moreItems({ step: 'point', forsaken: false, gameSent: false, capture: none });
    expect(other).toEqual([{ id: 'use-codes', label: 'Use Forsaken Lands prompt codes…' }]);
    const forget = items[3];
    expect(forget !== 'separator' && forget.danger).toBe(true);
  });

  it('holds nothing on the capture steps', () => {
    expect(moreItems({ step: 'codes', forsaken: true, gameSent: true, capture: none })).toEqual([]);
    expect(
      moreItems({ step: 'codes-entry', forsaken: true, gameSent: false, capture: none }),
    ).toEqual([]);
  });
});

describe('where the codes came from', () => {
  // 5:04 in the morning, local time.
  const at = new Date(2026, 8, 29, 5, 4);

  it('reads the time the way #prompt does, with no day half', () => {
    expect(clockTime(at)).toBe('5:04');
    expect(clockTime(new Date(2026, 8, 29, 12, 58))).toBe('12:58');
    expect(clockTime(new Date(2026, 8, 29, 0, 7))).toBe('12:07');
  });

  it('says the game sent them at login, or when', () => {
    const seen = { enabled: true, prompt: '%h ', fprompt: '' };
    expect(codesSourceLine({ ...seen, receivedAt: at.getTime(), atLogin: true })).toBe(
      'The game sent these codes when you logged in. Change them in the game and Vosh follows.',
    );
    expect(
      codesSourceLine({
        ...seen,
        receivedAt: new Date(2026, 8, 29, 12, 58).getTime(),
        atLogin: false,
      }),
    ).toBe('The game sent these codes at 12:58. Change them in the game and Vosh follows.');
    expect(codesSourceLine(null)).toBeNull();
  });

  it('says where Vosh saw your setting when the game sent none', () => {
    const today = new Date(2026, 8, 29, 9, 30);
    const seen = {
      prompt: '%h ',
      fprompt: '',
      enabled: true,
      at: '2026-09-29T05:04:00',
      at_login: false,
      source: 'log' as const,
      character: 'Tester',
    };
    expect(lastSeenLine(seen, today)).toBe('Vosh saw it when you typed prompt at 5:04.');
    expect(lastSeenLine({ ...seen, at: '2026-09-28T22:10:00' }, today)).toBe(
      'Vosh found it in your log from Sep 28.',
    );
    expect(lastSeenLine({ ...seen, at: null }, today)).toBeNull();
    expect(lastSeenLine(null, today)).toBeNull();
  });

  it('asks the question each state of the codes step asks', () => {
    expect(entryCopy('Vosh saw it when you typed prompt at 5:04.')).toEqual({
      title: 'Is this your prompt setting?',
      body: 'Vosh saw it when you typed prompt at 5:04. It reads the codes, so you never write a pattern.',
    });
    expect(entryCopy(null)).toEqual({
      title: 'What is your prompt setting?',
      body: 'Type prompt in the game and Vosh reads the answer. You can also paste it here.',
    });
    // Change codes… shows the codes the profile holds.
    expect(entryCopy(null, true)).toEqual({
      title: 'Is this your prompt setting?',
      body: 'Vosh reads the codes, so you never write a pattern.',
    });
  });

  it('saves the codes with where they came from', () => {
    const report = { prompt: '[%h] ', fprompt: '' };
    expect(savedCapture(report, 'gmcp', '2026-09-29T05:04:00-05:00')).toEqual({
      kind: 'aabahran',
      prompt: '[%h] ',
      fprompt: '',
      follow_game: true,
      seen_at: '2026-09-29T05:04:00-05:00',
      source: 'gmcp',
    });
  });

  it('leaves drawing off when the first capture saves, so only you turn it on', () => {
    const fresh: PromptConfig = {
      draw: false,
      template: '',
      previous_templates: [],
      capture: none,
      show: 'text',
      mirror: true,
    };
    expect(withCapture(fresh, codes)).toEqual({ ...fresh, capture: codes });
    expect(withCapture({ ...fresh, capture: migrated }, codes).draw).toBe(false);
    // A switch you turned on stays on through a new capture.
    const on = { ...fresh, draw: true, capture: codes };
    const other: PromptCapture = { ...codes, prompt: '<%hhp> ' };
    expect(withCapture(on, other)).toEqual({ ...on, capture: other });
  });
});

describe('the candidate box', () => {
  const read = (
    plain: string,
    marks: PromptCheckRead['marks'],
    fight = false,
  ): PromptCheckRead => ({
    id: 1,
    raw: plain.replace(/\n/g, '\r\n'),
    plain,
    at_ms: 0,
    fight,
    marks,
  });
  const mark = (line: number, start: number, end: number, label: string, warn = false) => ({
    line,
    start,
    end,
    field: warn ? null : label.toLowerCase(),
    label,
    warn,
  });
  // SF 11 widths of the labels, near enough.
  const widths: Record<string, number> = {
    Wizi: 21.6,
    Incog: 27.6,
    Health: 33.5,
    'Max health': 54.5,
    Mana: 28,
    'Max mana': 48.6,
    Moves: 33,
    'Max moves': 53.5,
    Tank: 23.5,
    'Tank health': 55,
    'Health and Mana': 80,
  };
  const measure = (label: string) => widths[label] ?? label.length * 6;

  it('starts each name under the first character it names, a crowded one a row lower', () => {
    const plain = '(Wizi 60) (Incog 60) [1020/1020hp 800/800mn 930/930mv]';
    const marks = [
      mark(0, 6, 8, 'Wizi'),
      mark(0, 17, 19, 'Incog'),
      mark(0, 22, 26, 'Health'),
      mark(0, 27, 31, 'Max health'),
      mark(0, 34, 37, 'Mana'),
      mark(0, 38, 41, 'Max mana'),
      mark(0, 44, 47, 'Moves'),
      mark(0, 48, 51, 'Max moves'),
    ];
    const placed = placeLabels(read(plain, marks), 7.8, measure);
    expect(placed.labels.map((l) => [l.label, Number(l.left.toFixed(1)), l.row])).toEqual([
      ['Wizi', 56.8, 0],
      ['Incog', 142.6, 0],
      ['Health', 181.6, 0],
      ['Max health', 220.6, 1],
      ['Mana', 275.2, 0],
      ['Max mana', 306.4, 1],
      ['Moves', 353.2, 0],
      ['Max moves', 384.4, 1],
    ]);
    expect(placed.labels[3].top).toBe(48.5);
    expect(placed.labels[0].top).toBe(31.5);
    expect(placed.lineTops).toEqual([10]);
    expect(boxHeight(placed)).toBe(74);
  });

  it('stacks the lines of a fight prompt with their names', () => {
    const plain =
      '(Wizi 60) (Incog 60) Tester: [===|---|---|---]\n[235/1020hp 800/800mn 930/930mv]';
    const marks = [
      mark(0, 6, 8, 'Wizi'),
      mark(0, 17, 19, 'Incog'),
      mark(0, 21, 27, 'Tank'),
      mark(0, 30, 45, 'Tank health'),
      mark(1, 1, 4, 'Health'),
      mark(1, 5, 9, 'Max health'),
      mark(1, 12, 15, 'Mana'),
      mark(1, 16, 19, 'Max mana'),
      mark(1, 22, 25, 'Moves'),
      mark(1, 26, 29, 'Max moves'),
    ];
    const placed = placeLabels(read(plain, marks, true), 7.8, measure);
    const rows = placed.labels.map((l) => [l.label, l.row, l.top]);
    expect(rows.slice(0, 4)).toEqual([
      ['Wizi', 0, 31.5],
      ['Incog', 0, 31.5],
      ['Tank', 1, 48.5],
      ['Tank health', 0, 31.5],
    ]);
    expect(rows[4]).toEqual(['Health', 0, 91]);
    expect(rows[5]).toEqual(['Max health', 1, 108]);
    expect(placed.lineTops).toEqual([10, 69.5]);
    expect(boxHeight(placed)).toBe(133.5);
  });

  it('gives a run Vosh cannot split one name, and moves the next name down when it crowds', () => {
    const placed = placeLabels(
      read('<1020800 930mv> ', [mark(0, 1, 8, 'Health and Mana', true), mark(0, 9, 12, 'Moves')]),
      7.8,
      measure,
    );
    expect(placed.labels.map((l) => [l.label, Number(l.left.toFixed(1)), l.row, l.warn])).toEqual([
      ['Health and Mana', 17.8, 0, true],
      ['Moves', 80.2, 1, false],
    ]);
    expect(boxHeight(placed)).toBe(74);
  });

  it('is one line tall with nothing to name', () => {
    const placed = placeLabels(read('You are hungry.', []), 7.8, measure);
    expect(boxHeight(placed)).toBe(38);
  });

  it('names the immortal prefix it reads', () => {
    const plain = '(Wizi 60) (Incog 55) [1020/1020hp]';
    expect(prefixNote(read(plain, [mark(0, 6, 8, 'Wizi'), mark(0, 17, 19, 'Incog')]))).toBe(
      'The game puts (Wizi 60) and (Incog 55) in front. Vosh reads those too.',
    );
    expect(prefixNote(read('(Wizi 60) [1020hp]', [mark(0, 6, 8, 'Wizi')]))).toBe(
      'The game puts (Wizi 60) in front. Vosh reads that too.',
    );
    expect(prefixNote(read('[1020hp]', [mark(0, 1, 5, 'Health')]))).toBeNull();
  });

  it('says the codes replace the pattern from your old trigger', () => {
    expect(migratedNote(migrated)).toBe('This replaces the pattern from your old capture trigger.');
    expect(migratedNote(codes)).toBeNull();
    expect(migratedNote(none)).toBeNull();
  });
});

describe('the match line', () => {
  const check = (patch: Partial<PromptCaptureCheck>): PromptCaptureCheck => ({
    matched: 14,
    total: 14,
    fight_matched: 0,
    false_matches: 0,
    text: 'Matches your last 14 prompts and no other line.',
    reads: [],
    ...patch,
  });

  it('puts each sentence on its own line', () => {
    expect(
      matchSentences('Matches your last 14 prompts and no other line. 3 of them are from a fight.'),
    ).toEqual(['Matches your last 14 prompts and no other line.', '3 of them are from a fight.']);
    expect(matchSentences('Matches your last prompt and no other line.')).toEqual([
      'Matches your last prompt and no other line.',
    ]);
  });

  it('wraps the empty ring copy as one paragraph (P0)', () => {
    const empty =
      'Vosh has not seen your prompt since you connected. Send a command and Vosh checks again.';
    expect(matchLines(check({ matched: 0, total: 0, text: empty }))).toEqual([empty]);
    expect(
      matchLines(
        check({
          text: 'Matches your last 14 prompts and no other line. 3 of them are from a fight.',
        }),
      ),
    ).toEqual(['Matches your last 14 prompts and no other line.', '3 of them are from a fight.']);
  });

  it('checks a clean match, warns about a poor one, and marks nothing with no prompt yet', () => {
    expect(matchTone(check({}))).toBe('ok');
    expect(matchTone(check({ false_matches: 2 }))).toBe('warn');
    expect(matchTone(check({ matched: 0 }))).toBe('warn');
    expect(matchTone(check({ matched: 0, total: 0 }))).toBe('none');
  });
});

describe('the start list', () => {
  const preset = (id: PromptPreset['id'], label: string, template: string): PromptPreset => ({
    id,
    label,
    template,
  });
  const presets = [
    preset('default', "Vosh's default", 'DEFAULT '),
    preset('game', 'Same as the game', 'GAME '),
    preset('minimal', 'Minimal', 'MIN '),
    preset('detailed', 'Detailed', 'DETAILED '),
    preset('empty', 'Start empty', ''),
  ];
  const config = (template: string, previous: string[], mirror = false): PromptConfig => ({
    draw: true,
    template,
    previous_templates: previous,
    capture: codes,
    show: 'text',
    mirror,
  });

  it('lists the default, yours, the presets and Start empty, yours checked', () => {
    const list = startRows(presets, config('MINE', ['MINE']), []);
    expect(list.rows.map((r) => [r.label, r.checked])).toEqual([
      ["Vosh's default", false],
      ['Yours', true],
      ['Same as the game', false],
      ['Minimal', false],
      ['Detailed', false],
    ]);
    expect(list.empty).toEqual({
      id: 'empty',
      label: 'Start empty',
      template: '',
      checked: false,
    });
    expect(list.others).toEqual([]);
  });

  it('adds your design before that under yours once there is one', () => {
    const list = startRows(presets, config('MIN ', ['MIN ', 'MINE']), []);
    expect(list.rows.map((r) => r.label).slice(0, 3)).toEqual([
      "Vosh's default",
      'Yours',
      'Your design before that',
    ]);
    // Minimal is yours now, so the first row that holds it takes the check.
    expect(list.rows.filter((r) => r.checked).map((r) => r.label)).toEqual(['Yours']);
  });

  it('leaves out an earlier design that is the default or the same as yours', () => {
    const list = startRows(presets, config('DEFAULT ', ['DEFAULT ', 'DEFAULT ']), []);
    expect(list.rows.map((r) => r.label)).toEqual([
      "Vosh's default",
      'Same as the game',
      'Minimal',
      'Detailed',
    ]);
    expect(list.rows[0].checked).toBe(true);
  });

  it('offers the designs other profiles hold', () => {
    const list = startRows(presets, config('MINE', ['MINE']), [
      { profile: 'Healer', display_name: 'Healer', template: '[%hp]' },
    ]);
    expect(list.others).toEqual([
      { id: 'profile:Healer', label: "Healer's prompt", template: '[%hp]', checked: false },
    ]);
  });

  it('checks Start empty while the design is empty', () => {
    expect(startRows(presets, config('', []), []).empty?.checked).toBe(true);
  });

  it('checks Same as the game while your design follows the game', () => {
    const list = startRows(presets, config('GAME ', ['MINE'], true), []);
    expect(list.rows.filter((r) => r.checked).map((r) => r.label)).toEqual(['Same as the game']);
    // With no codes to follow the design is empty, and no row is
    // checked, Start empty included.
    const none = startRows(presets, config('', [], true), []);
    expect(none.rows.some((r) => r.checked)).toBe(false);
    expect(none.empty?.checked).toBe(false);
    // A design of yours that reads as the game does is yours, so Same as
    // the game stays unchecked.
    const yours = startRows(presets, config('GAME ', []), []);
    expect(yours.rows.some((r) => r.checked)).toBe(false);
  });
});

describe('what a start and an edit save', () => {
  const table: PromptConfig = {
    draw: false,
    template: '[%hp] ',
    previous_templates: [],
    capture: { kind: 'aabahran', prompt: '[%hhp] ', fprompt: '', follow_game: true },
    show: 'pinned',
    mirror: true,
  };

  it('turns drawing on, and only Same as the game follows the game', () => {
    expect(withStart(table, { id: 'game', template: '[%hp] ' })).toEqual({
      ...table,
      draw: true,
      mirror: true,
    });
    for (const [id, template] of [
      ['default', 'DEFAULT '],
      ['minimal', 'MIN '],
      ['yours', 'MINE'],
      ['profile:Healer', '[%mana]'],
      ['empty', ''],
    ]) {
      expect(withStart(table, { id, template }), id).toEqual({
        ...table,
        template,
        draw: true,
        mirror: false,
      });
    }
  });

  it('makes the design yours with any edit, starting from the text it edited', () => {
    expect(withDesign(table, '[%s_bold%hp] ')).toEqual({
      ...table,
      template: '[%s_bold%hp] ',
      mirror: false,
    });
  });

  it('takes an edit back to following the game', () => {
    const edited = withDesign(table, '[%s_bold%hp] ');
    const entry = undoEntry(table, edited);
    expect(entry).toEqual({ template: '[%hp] ', mirror: true });
    expect(takeBackOnto(edited, entry!)).toEqual(table);
  });

  it('changes only where your prompt shows, and Command Z keeps the place', () => {
    for (const show of ['text', 'lifted'] as const) {
      const placed = withShow(table, show);
      expect(placed, show).toEqual({ ...table, show });
      // The card moves with your prompt, so taking a change back never
      // moves it again.
      expect(undoEntry(table, placed), show).toBeNull();
    }
    // A design change after it takes back the design alone.
    const lifted = withShow(table, 'lifted');
    const edited = withDesign(lifted, '[%s_bold%hp] ');
    expect(takeBackOnto(edited, undoEntry(lifted, edited)!)).toEqual(lifted);
  });

  it('takes a move back with the other Option key to following the game', () => {
    // Option with Right moves a part of a design that follows the game,
    // which makes the design yours. The move keeps what the design was.
    const after = '[ %hp]';
    const moved = withDesign(table, after);
    expect(moved.mirror).toBe(false);
    const right: MoveMade = {
      before: table.template,
      after,
      from: 1,
      landed: 2,
      dir: 1,
      mirror: table.mirror,
    };
    // Option with Left takes it back, and the design follows the game
    // again, as Command Z does.
    const back = moveBack([right], moved.template, 2, -1);
    expect(back).toEqual(right);
    expect(withMoveTakenBack(moved, back!)).toEqual(table);
    expect(withMoveTakenBack(moved, back!)).toEqual(takeBackOnto(moved, undoEntry(table, moved)!));
    // A move of a design of yours comes back yours.
    const yours = { ...table, mirror: false };
    expect(withMoveTakenBack(withDesign(yours, after), { ...right, mirror: false })).toEqual(yours);
  });

  it('lands an edit on the table as it stands, so a place picked meanwhile stays', () => {
    // The edit began on the table, and while it waited on its round
    // trips you picked Lifted at the foot and turned drawing on.
    const now = { ...withShow(table, 'lifted'), draw: true };
    const edited = editedTable(table, now, '[%s_bold%hp] ');
    expect(edited).toEqual({ ...now, template: '[%s_bold%hp] ', mirror: false });
    // Command Z takes back the design alone.
    expect(takeBackOnto(edited!, undoEntry(now, edited!)!)).toEqual(now);
    // An edit that left the design as it was saves nothing.
    expect(editedTable(table, now, table.template)).toBeNull();
    // With no table to land on, it lands on the one it began on.
    expect(editedTable(table, null, '[%s_bold%hp] ')).toEqual(withDesign(table, '[%s_bold%hp] '));
  });

  it('lands a move taken back on the table as it stands too', () => {
    const after = '[ %hp]';
    const moved = withDesign(table, after);
    const right: MoveMade = {
      before: table.template,
      after,
      from: 1,
      landed: 2,
      dir: 1,
      mirror: table.mirror,
    };
    // You picked In the text while the move went back.
    const now = withShow(moved, 'text');
    expect(movedBackTable(moved, now, right)).toEqual({ ...table, show: 'text' });
    expect(movedBackTable(moved, null, right)).toEqual(table);
  });
});

describe('the button at the foot that says where your prompt shows', () => {
  const reads: PromptShowState = {
    show: 'pinned',
    capture: true,
    draw: true,
    gameSent: true,
    zone: 1,
    promptsOff: false,
  };

  it('waits quietly while the card holds a capture the state has yet to read', () => {
    // Right after your first capture the card's table holds it, and the
    // state reads it a round trip later. The button is off meanwhile and
    // never asks you to customize the prompt you are customizing.
    const behind = { ...reads, capture: false };
    expect(cardShowState(behind, codes)).toBeNull();
    expect(promptShowLock(cardShowState(behind, codes))).toEqual({ locked: true, why: null });
    // Once the state reads the capture, the button follows it.
    expect(cardShowState(reads, codes)).toBe(reads);
    expect(cardShowState(null, codes)).toBeNull();
    // With no capture in the card either, it keeps the reason.
    expect(cardShowState(behind, none)).toBe(behind);
  });
});

describe('naming the numbers of another game', () => {
  const number = (text: string, name: string, label: string, max = false): PromptLineNumber => ({
    span: [0, 0],
    text,
    name,
    suggested: name,
    label,
    max,
  });
  const numbers = [
    number('1020', 'hp', 'Health'),
    number('1020', 'maxhp', 'Max health', true),
    number('800', 'mana', 'Mana'),
    number('800', 'maxmana', 'Max mana', true),
    number('7', 'n1', 'n1'),
  ];

  it('gives each pair one button named for its value, and a lone number its own', () => {
    expect(numberButtons(numbers).map((b) => [b.index, b.label, b.aria])).toEqual([
      [0, 'Health', '1020 of 1020 is Health'],
      [2, 'Mana', '800 of 800 is Mana'],
      [4, 'Name this number', '7 is not named'],
    ]);
  });

  it('names a pair and its max together, or leaves both out', () => {
    expect(namesFor(numbers, 0, 'move')).toEqual(['move', 'maxmove', 'mana', 'maxmana', 'n1']);
    expect(namesFor(numbers, 2, '')).toEqual(['hp', 'maxhp', '', '', 'n1']);
    expect(namesFor(numbers, 2, 'mp')).toEqual(['hp', 'maxhp', 'mp', 'maxmp', 'n1']);
    expect(namesFor(numbers, 4, 'gold')).toEqual(['hp', 'maxhp', 'mana', 'maxmana', 'gold']);
  });

  it('lays the buttons out as P15 draws them, and stacks the ones that would touch', () => {
    // Another game: three pairs far apart, one row 35.5 down a box 66
    // tall.
    const width = (label: string) => ({ Health: 33.5, Mana: 26.4, Moves: 31 })[label] ?? 82;
    const apart = placeNameButtons(
      [
        { index: 0, col: 1, label: 'Health' },
        { index: 2, col: 13, label: 'Mana' },
        { index: 4, col: 22, label: 'Moves' },
      ],
      7.8,
      width,
    );
    expect(apart.height).toBe(66);
    expect(apart.buttons.map((b) => [b.index, Number(b.left.toFixed(1)), b.top])).toEqual([
      [0, 11.8, 35.5],
      [2, 105.4, 35.5],
      [4, 175.6, 35.5],
    ]);
    // An immortal's Wizi and Incog sit close to the health: the Incog
    // button takes a second row 22 under the first, and the box grows.
    const close = placeNameButtons(
      [
        { index: 0, col: 6, label: 'Name this number' },
        { index: 1, col: 17, label: 'Name this number' },
        { index: 2, col: 21, label: 'Health' },
        { index: 4, col: 33, label: 'Mana' },
      ],
      7.8,
      width,
    );
    expect(close.buttons.map((b) => [b.index, b.top])).toEqual([
      [0, 35.5],
      [1, 57.5],
      [2, 35.5],
      [4, 35.5],
    ]);
    expect(close.height).toBe(88);
    // No two buttons on a row come within 4 px of each other.
    const rows = new Map<number, { left: number; right: number }[]>();
    for (const b of close.buttons) rows.set(b.top, [...(rows.get(b.top) ?? []), b]);
    for (const row of rows.values()) {
      for (let i = 1; i < row.length; i++)
        expect(row[i].left).toBeGreaterThanOrEqual(row[i - 1].right + 4);
    }
  });

  it('offers the vitals Vosh knows, then what the game sends, then a name of your own', () => {
    expect(nameChoices([{ name: 'mp', package: 'Char.Vitals' }])).toEqual([
      [
        { name: 'hp', label: 'Health' },
        { name: 'mana', label: 'Mana' },
        { name: 'move', label: 'Moves' },
      ],
      [{ name: 'mp', label: 'mp', package: 'Char.Vitals' }],
    ]);
  });
});

describe('where the card sits', () => {
  it('ends 4 px above the row right above your prompt, as the boards draw it', () => {
    // A window with rows 17.5 tall from y 38, the prompt on row 38.
    const anchor = cardAnchor({
      pinned: false,
      promptTop: 38 + 38 * 17.5,
      lastRowTop: 38 + 38 * 17.5,
      bandRowTop: null,
      cellH: 17.5,
      areaTop: 32,
      viewportH: 800,
    });
    expect(anchor.bottom).toBe(800 - 681.5);
    expect(anchor.maxHeight).toBe(681.5 - 40);
  });

  it('sits over the last row with no open row', () => {
    const anchor = cardAnchor({
      pinned: false,
      promptTop: null,
      lastRowTop: 703,
      bandRowTop: null,
      cellH: 17.5,
      areaTop: 32,
      viewportH: 800,
    });
    expect(anchor.bottom).toBe(800 - 681.5);
  });

  it('ends 4 px above the pinned band', () => {
    const anchor = cardAnchor({
      pinned: true,
      promptTop: null,
      lastRowTop: 703,
      bandRowTop: 712,
      cellH: 17.5,
      areaTop: 32,
      viewportH: 800,
    });
    expect(anchor.bottom).toBe(800 - 708);
  });
});

describe('the legend and the box lines', () => {
  it('fills the first column first, with the odd row', () => {
    expect(legendColumns([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])).toEqual([
      [1, 2, 3, 4, 5],
      [6, 7, 8, 9, 10],
    ]);
    expect(legendColumns(['%h%m', '%v'])).toEqual([['%h%m'], ['%v']]);
    expect(legendColumns([1, 2, 3])).toEqual([[1, 2], [3]]);
  });

  it('reads the raw lines as cells and the marks as their columns', () => {
    const read = {
      id: 1,
      raw: '\x1b[38;5;240m(Wizi \x1b[0m60\x1b[38;5;240m)\x1b[0m [10/20hp]\r\n[\u4e2d5hp]',
      plain: '(Wizi 60) [10/20hp]\n[\u4e2d5hp]',
      at_ms: 0,
      fight: false,
      marks: [
        { line: 0, start: 6, end: 8, field: 'wizi', label: 'Wizi', warn: false },
        { line: 1, start: 2, end: 3, field: 'hp', label: 'Health', warn: false },
      ],
    };
    const rows = readRows(read);
    expect(rows).toHaveLength(2);
    expect(rows[0][0].attrs.fg).toEqual({ kind: 'indexed', n: 240 });
    // The wide character takes two cells, so the 5 sits in the fourth.
    expect(readMarks(read)).toEqual([
      [{ from: 6, to: 8, warn: false }],
      [{ from: 3, to: 4, warn: false }],
    ]);
  });
});

describe('Command Z', () => {
  const table: PromptConfig = {
    draw: true,
    template: '[%hp]',
    previous_templates: [],
    capture: { kind: 'aabahran', prompt: '<%hhp> ', fprompt: '', follow_game: true },
    show: 'text',
    mirror: false,
  };

  it('takes back only what your change made, onto the table as it stands now', () => {
    const bold: PromptConfig = { ...table, template: '[%s_bold%hp]' };
    const entry = undoEntry(table, bold);
    expect(entry).toEqual({ template: '[%hp]' });
    // The game sent new codes and you chose Pinned in Settings since.
    const now: PromptConfig = {
      ...bold,
      capture: { kind: 'aabahran', prompt: '<%hhp %mm> ', fprompt: '', follow_game: true },
      show: 'pinned',
    };
    expect(takeBackOnto(now, entry!)).toEqual({ ...now, template: '[%hp]' });
  });

  it('takes back the switch, and a preset with the switch it turned on', () => {
    expect(undoEntry(table, { ...table, draw: false })).toEqual({ draw: true });
    const off = { ...table, draw: false };
    const preset = { ...off, template: '%hp ', draw: true };
    expect(undoEntry(off, preset)).toEqual({ template: '[%hp]', draw: false });
  });

  it('keeps nothing for a change that changed nothing', () => {
    expect(undoEntry(table, { ...table })).toBeNull();
  });
});

describe('a request to open the card', () => {
  it('counts each one, so the open card hears the same request again', () => {
    const first = nextCardRequest(null, 'design');
    expect(first).toEqual({ view: 'design', at: 1 });
    const text = nextCardRequest(first, 'text');
    expect(text).toEqual({ view: 'text', at: 2 });
    // Edit prompt as text… again, after you went back to the parts.
    expect(nextCardRequest(text, 'text')).toEqual({ view: 'text', at: 3 });
  });
});

describe('a preset sample cut to its column', () => {
  // JetBrains Mono at 13 is 7.8 wide. Each sample cuts as a column with
  // text-overflow ellipsis does.
  it('keeps what fits with its ellipsis, as P4 and the Presets menu cut', () => {
    // The start list's 429 column: Detailed's 59 cells end "2 m…".
    expect(sampleCut(59, 7.8, 429)).toEqual({ kept: 53, width: 429 });
    // The Presets menu's 410: "930/930…".
    expect(sampleCut(59, 7.8, 410)).toEqual({ kept: 51, width: 410 });
    // A fight line leaves its tag room: 356.09 keeps 44.
    expect(sampleCut(60, 7.8, 356.09)).toEqual({ kept: 44, width: 356.09 });
  });

  it('shows a sample that fits whole, at its own width', () => {
    expect(sampleCut(55, 7.8, 429)).toEqual({ kept: 55, width: 429 });
    expect(sampleCut(18, 7.8, 410)).toEqual({ kept: 18, width: 140.4 });
  });
});
