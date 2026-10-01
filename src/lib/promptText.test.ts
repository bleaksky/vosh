import { describe, expect, it } from 'vitest';
import {
  fieldHtml,
  insertAt,
  oneLine,
  pieceAtCaret,
  pieceRange,
  TEXT_HELP,
  TOKEN_ROWS,
  tokenTone,
  unknownTitle,
} from './promptText';
import type { PromptToken } from './session';

// `[%c_hp%hp %nope` as prompt_describe reads it.
const TOKENS: PromptToken[] = [
  { start: 0, end: 1, piece: 0, kind: 'text', name: null, known: true },
  { start: 1, end: 6, piece: 1, kind: 'code', name: null, known: true },
  { start: 6, end: 9, piece: 1, kind: 'value', name: 'hp', known: true },
  { start: 9, end: 10, piece: 2, kind: 'text', name: null, known: true },
  { start: 10, end: 15, piece: 3, kind: 'value', name: 'nope', known: false },
];
const TEXT = '[%c_hp%hp %nope';

describe('Edit as text', () => {
  it('colors each token by what it is', () => {
    expect(TOKENS.map(tokenTone)).toEqual(['text', 'code', 'value', 'text', 'unknown']);
    expect(unknownTitle('nope')).toBe('Vosh has no value called nope.');
  });

  it('marks the part the token at the caret draws', () => {
    // After %hp, the hp part with its color code.
    expect(pieceAtCaret(TOKENS, 9)).toBe(1);
    expect(pieceRange(TOKENS, 1)).toEqual({ start: 1, end: 9 });
    // Inside the color code, the same part.
    expect(pieceAtCaret(TOKENS, 3)).toBe(1);
    expect(pieceAtCaret(TOKENS, 0)).toBe(0);
    expect(pieceAtCaret(TOKENS, 15)).toBe(3);
    expect(pieceAtCaret([], 0)).toBeNull();
  });

  it('adds a token at the caret, over what is selected', () => {
    expect(insertAt(TEXT, 9, 9, '%s_bold')).toEqual({
      text: '[%c_hp%hp%s_bold %nope',
      caret: 16,
    });
    expect(insertAt(TEXT, 10, 15, '%gold')).toEqual({ text: '[%c_hp%hp %gold', caret: 15 });
    expect(oneLine('a\nb\r\nc')).toBe('abc');
  });

  it('draws the field with break chances only between tokens', () => {
    expect(fieldHtml(TEXT, TOKENS, 1)).toBe(
      '<span class="pc-tok is-text">[</span><wbr>' +
        '<span class="pc-text-mark"><span class="pc-tok is-code">%c_hp</span><wbr>' +
        '<span class="pc-tok is-value">%hp</span></span><wbr>' +
        '<span class="pc-tok is-text"> </span><wbr>' +
        '<span class="pc-tok is-unknown" title="Vosh has no value called nope.">%nope</span>',
    );
    // Text past the tokens, while you type, shows as it is.
    expect(fieldHtml('<b>', [], null)).toBe('&lt;b&gt;');
  });

  it('offers the boards token rows and help in plain sentences', () => {
    expect(TOKEN_ROWS.map((r) => r.label)).toEqual(['Forms', 'Color', 'Style', 'Layout']);
    expect(TOKEN_ROWS[3].tokens).toEqual(['%nl', '%{if:fight}', '%{end}']);
    expect(TEXT_HELP).not.toMatch(/[;:–—]| - /);
  });
});
