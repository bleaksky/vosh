import { describe, expect, it } from 'vitest';
import { warnBoxes, warnedPieces } from './promptWarn';
import type { PromptFieldState } from '../ipc/prompt';
import type { PromptPiece, PromptSpan, PromptToken } from '../ipc/promptDesign';

// The parts of your design no value fills, which the card and Settings
// ring in --warn.

const field = (over: Partial<PromptFieldState>): PromptFieldState => ({
  name: 'hp',
  label: 'Health',
  aliases: [],
  kind: 'gauge',
  group: 'vitals',
  package: null,
  new_build: false,
  codes: [],
  search: [],
  param: false,
  listed: true,
  state: 'value',
  source: null,
  value: null,
  max: null,
  sent: true,
  in_prompt: false,
  ...over,
});

const piece = (over: Partial<PromptPiece>): PromptPiece =>
  ({ piece: 0, kind: 'value', field: null, shows: true, ...over }) as PromptPiece;

describe('the parts no value fills', () => {
  // On an older build only %P sends your tank's health.
  const catalog = [
    field({ name: 'hp', package: 'Char.Vitals', codes: ['%h'], in_prompt: true }),
    field({
      name: 'tank_hp',
      label: 'Tank health',
      group: 'fight',
      package: 'Char.Combat',
      new_build: true,
      sent: false,
      codes: ['%p', '%P'],
      state: 'missing',
    }),
  ];

  it('rings a part your prompt no longer feeds and a name Vosh does not know', () => {
    const pieces = [
      piece({ piece: 0, field: 'tank_hp' }),
      piece({ piece: 1, field: 'hp' }),
      piece({ piece: 2, kind: 'unknown' }),
      piece({ piece: 3, kind: 'if', shows: false, field: null }),
    ];
    const tokens = [{ piece: 2, known: false }] as PromptToken[];
    expect([...warnedPieces(pieces, tokens, catalog)]).toEqual([0, 2]);
  });

  it('leaves a part a package still sends', () => {
    const sent = catalog.map((f) =>
      f.name === 'tank_hp' ? { ...f, sent: true, state: 'value' as const } : f,
    );
    expect(warnedPieces([piece({ field: 'tank_hp' })], [], sent).size).toBe(0);
  });

  it('puts the ring round the cells each ringed part draws', () => {
    const spans = [
      { piece: 0, row: 0, col: 8, width: 11 },
      { piece: 1, row: 1, col: 0, width: 4 },
    ] as PromptSpan[];
    expect(warnBoxes(spans, new Set([0]), { x: 10, y: 5.25, cellW: 7.8, rowH: 17.5 })).toEqual([
      { left: 72.4, top: 5.25, width: 85.8, height: 17.5 },
    ]);
  });
});
