import { describe, expect, it } from 'vitest';
import { parseAnsi, styleToCss } from './ansi';
import { promptPreviewChunks, promptPreviewVars, SAMPLE_VITALS } from './promptPreview';

// James's template from the Input board.
const TEMPLATE =
  '%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset';

const DESIGN =
  '%{if:fight}%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% %{c:245}%opponent_cond%c_default%nl%{end}%{if:hp}%{if:maxhp}%c_hp%{end}%hp%{c:245}%{if:maxhp}/%{maxhp}%{end}hp%c_default%{end} ';

describe('promptPreviewVars', () => {
  it('fills every vital to the sample when Vosh has not heard yours', () => {
    const vars = promptPreviewVars(null);
    expect(vars.hp).toBe('1020');
    expect(vars.maxhp).toBe('1020');
    expect(vars.mana).toBe('800');
    expect(vars.mv).toBe('930');
    expect(vars.maxmv).toBe('930');
  });

  it('fills every vital to your live max', () => {
    const vars = promptPreviewVars({ ...SAMPLE_VITALS, hp: 12, maxhp: 1400, maxmana: 0 });
    expect(vars.hp).toBe('1400');
    expect(vars.maxhp).toBe('1400');
    // A vital your MUD does not send keeps the sample.
    expect(vars.mana).toBe('800');
  });
});

describe('promptPreviewChunks', () => {
  it('draws the board template with its colors and italics', () => {
    const chunks = promptPreviewChunks(TEMPLATE, null);
    expect(chunks.map((c) => c.text).join('')).toBe('[1020(100%)h 800(100%)m 930(100%)v] ');
    const bracket = chunks[0];
    expect(bracket.text).toBe('[');
    expect(bracket.style.fg).toBe('rgb(100,100,100)');
    const hp = chunks.find((c) => c.text === '100');
    // %c_hp at full health is the 256 color 42.
    expect(hp?.style.fg).toBe('rgb(0,215,135)');
    expect(hp?.style.italic).toBe(true);
    const mana = chunks.filter((c) => c.text === '100')[1];
    expect(mana?.style.fg).toBe('rgb(128,200,255)');
  });

  it('draws nothing for an empty template', () => {
    expect(promptPreviewChunks('', null)).toEqual([]);
  });

  it('draws nothing for a design with a condition, a line break or the raw prompt', () => {
    // The start of Vosh's default design. Only the terminal draws these
    // forms, and the preview would print them as typed.
    expect(promptPreviewChunks(DESIGN, null)).toEqual([]);
    expect(promptPreviewChunks('%hp%nl%mana', null)).toEqual([]);
    expect(promptPreviewChunks('%hp%{nl}%mana', null)).toEqual([]);
    expect(promptPreviewChunks('%{ifnot:fight}%hp%{end}', null)).toEqual([]);
    expect(promptPreviewChunks('%{raw}', null)).toEqual([]);
    // A name the preview does not know still prints as typed, so you spot
    // the typo.
    expect(
      promptPreviewChunks('%hpp end', null)
        .map((c) => c.text)
        .join(''),
    ).toBe('%hpp end');
  });
});

describe('parseAnsi styles', () => {
  it('reads dim, italic, and strike and their resets', () => {
    const chunks = parseAnsi('\x1b[2;3;9ma\x1b[22;23;29mb');
    expect(chunks[0].style).toMatchObject({ dim: true, italic: true, strike: true });
    expect(chunks[1].style).toMatchObject({ dim: false, italic: false, strike: false });
  });

  it('ends bold and dim together on 22', () => {
    const [, after] = parseAnsi('\x1b[1;2mx\x1b[22my');
    expect(after.style.bold).toBe(false);
    expect(after.style.dim).toBe(false);
  });

  it('turns the styles into CSS', () => {
    expect(styleToCss({ italic: true, dim: true })).toEqual({ fontStyle: 'italic', opacity: 0.6 });
    expect(styleToCss({ underline: true, strike: true }).textDecoration).toBe(
      'underline line-through',
    );
    expect(styleToCss({ strike: true }).textDecoration).toBe('line-through');
  });
});
