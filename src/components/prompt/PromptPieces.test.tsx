import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { BandEnv } from '../../lib/bandCells';
import { BY_VALUE_HINT, THEME_HINT, WHEN_FIXED_HINT } from '../../lib/promptPieces';
import { TEXT_HELP } from '../../lib/promptText';
import type { PromptFieldState, PromptForm, PromptPiece, PromptState } from '../../lib/session';
import { LineTriggers } from './PromptCodes';
import { PromptPicker } from './PromptPicker';
import { PromptPieceBody } from './PromptPiece';
import { Starts } from './PromptStarts';
import { PromptText } from './PromptText';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// Nord's terminal colors.
const NORD: BandEnv = {
  palette: [
    '#3b4252',
    '#bf616a',
    '#a3be8c',
    '#ebcb8b',
    '#81a1c1',
    '#b48ead',
    '#88c0d0',
    '#e5e9f0',
    '#4c566a',
    '#bf616a',
    '#a3be8c',
    '#ebcb8b',
    '#81a1c1',
    '#b48ead',
    '#8fbcbb',
    '#eceff4',
  ],
  fg: '#e5e9f0',
  bg: '#2e3440',
  renderer: 'xterm',
  brightBold: false,
};

const form = (format: PromptForm['format'], segment: string): PromptForm => ({
  format,
  label: segment,
  segment,
  sample: { ansi: segment, plain: segment, rows: 1, spans: [] },
  show_as: true,
});

// P5: the hp value of his template, italic from the %s_italic before it.
const HP: PromptPiece = {
  piece: 3,
  kind: 'value',
  text: '%c_reset%s_italic%hp',
  field: 'hp',
  label: 'Health',
  format: 'value',
  width: null,
  when: 'always',
  when_fixed: false,
  color: { kind: 'default' },
  bold: false,
  italic: true,
  underline: false,
  literal: null,
  meta: '1020 of 1020',
  forms: [
    form('value', '1020'),
    form('cur_max', '1020/1020'),
    form('percent', '100%'),
    form('bar', 'Bar'),
  ],
  by_value: true,
  shows: true,
};

const draw = (piece: PromptPiece) =>
  renderToStaticMarkup(
    <PromptPieceBody piece={piece} env={NORD} onEdit={() => {}} onInsertValue={() => {}} />,
  );

/** The labels of the pressed buttons of a group, in order. */
function pressed(html: string, group: string): string[] {
  const start = html.indexOf(`aria-label="${group}"`);
  if (start < 0) return [];
  const end = html.indexOf('</div>', html.indexOf('st-seg', start));
  const part = html.slice(start, end);
  return [...part.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);
}

describe('a picked part', () => {
  it('names the part, reads its value and shows its own codes (P5)', () => {
    const html = draw(HP);
    expect(html).toContain('class="pc-piece-title">Health<');
    expect(html).toContain('class="pc-piece-meta">1020 of 1020<');
    expect(html).toContain('class="pc-piece-code">%c_reset%s_italic%hp<');
    expect(pressed(html, 'Show as')).toEqual(['1020']);
    expect(pressed(html, 'When')).toEqual(['Always']);
    // Its effective look: the terminal's text color and italic.
    expect(html).toMatch(/aria-label="Terminal text"[^>]*aria-pressed="true"/);
    expect(html).toMatch(/aria-label="By value"[^>]*aria-pressed="false"/);
    expect(html).toMatch(/aria-label="Italic" aria-pressed="true"/);
    expect(html).toMatch(/aria-label="Bold" aria-pressed="false"/);
    expect(html).toContain(THEME_HINT);
    expect(html).not.toContain('Width');
    expect(html).toContain('Insert value…');
    expect(html).toContain('>Remove<');
    // The theme swatches in board order, in the theme's colors.
    const swatches = [...html.matchAll(/aria-label="(Theme [a-z]+)"[^>]*background:([^"]*)"/g)].map(
      (m) => [m[1], m[2]],
    );
    expect(swatches).toEqual([
      ['Theme red', '#bf616a'],
      ['Theme green', '#a3be8c'],
      ['Theme yellow', '#ebcb8b'],
      ['Theme blue', '#81a1c1'],
      ['Theme magenta', '#b48ead'],
      ['Theme cyan', '#88c0d0'],
      ['Theme gray', '#4c566a'],
    ]);
  });

  it('gives a bar Width in place of Style, By value pressed (P7)', () => {
    const html = draw({
      ...HP,
      text: '%hp_bar:10',
      format: 'bar',
      width: 10,
      italic: false,
      color: { kind: 'by_value' },
    });
    expect(pressed(html, 'Show as')).toEqual(['Bar']);
    expect(html).toContain('>Width<');
    expect(html).toContain('value="10"');
    expect(html).toContain('>cells<');
    expect(html).not.toContain('aria-label="Italic"');
    expect(html).toMatch(/aria-label="By value"[^>]*aria-pressed="true"/);
    // By value's rule wraps in the control column.
    expect(html).toContain(`class="pc-piece-hint">${BY_VALUE_HINT.replace(/'/g, '&#x27;')}<`);
  });

  it('gives a line break only When, with what it means (P10)', () => {
    const html = draw({
      ...HP,
      piece: 5,
      kind: 'nl',
      text: '%nl',
      field: null,
      label: 'Line break',
      format: null,
      when: 'fight',
      meta: null,
      forms: [],
      by_value: false,
      shows: false,
    });
    expect(html).toContain('class="pc-piece-title">Line break<');
    expect(pressed(html, 'When')).toEqual(['In a fight']);
    expect(html).toContain(
      'The line above shows only in a fight, so out of a fight your prompt is one line.',
    );
    expect(html).not.toContain('Show as');
    expect(html).not.toContain('Terminal text');
    expect(html).not.toContain('aria-label="Bold"');
  });

  it('gives text its words in the terminal face', () => {
    const html = draw({
      ...HP,
      kind: 'text',
      text: '%{c:100,100,100}[',
      field: null,
      label: 'Text',
      format: null,
      literal: '[',
      meta: null,
      forms: [],
      by_value: false,
      color: { kind: 'rgb', r: 100, g: 100, b: 100 },
    });
    expect(html).toContain('>Text<');
    expect(html).toContain('st-field-mono');
    expect(html).toContain('value="["');
    expect(html).not.toContain('Show as');
    expect(html).not.toContain('aria-label="By value"');
    // A color no swatch names fills Custom with its hex.
    expect(html).toContain('value="#646464"');
  });

  it('holds When still for a part another condition decides', () => {
    const html = draw({ ...HP, when: 'fight', when_fixed: true });
    expect(html).toContain(WHEN_FIXED_HINT);
    const when = html.slice(html.indexOf('aria-label="When"'));
    expect(when).toMatch(/disabled=""[^>]*>Always</);
  });
});

/** A catalog field as prompt_state_get reports it. */
function field(name: string, over: Partial<PromptFieldState> = {}): PromptFieldState {
  return {
    name,
    label: name,
    aliases: [],
    kind: 'num',
    group: 'vitals',
    gmcp: null,
    package: null,
    new_build: false,
    codes: [],
    search: [],
    param: false,
    listed: true,
    formats: ['value'],
    state: 'missing',
    source: null,
    value: null,
    max: null,
    sent: true,
    in_prompt: false,
    ...over,
  };
}

describe('the picker', () => {
  it('lists the topics with what each field reads, Health first and highlighted (P6)', () => {
    const state: PromptState = {
      catalog: [
        field('hp', {
          label: 'Health',
          codes: ['%h'],
          package: 'Char.Vitals',
          state: 'value',
          value: '1020',
          in_prompt: true,
        }),
        field('opponent', { label: 'Opponent', group: 'fight', state: 'absent' }),
        field('pos', {
          label: 'Position',
          group: 'fight',
          codes: ['%S'],
          package: 'Char.State',
          new_build: true,
          state: 'value',
          value: 'standing',
        }),
      ],
      status: { status: 'matching', last_match_at: null },
      new_build: true,
      forsaken: true,
      open_row: null,
      packages: ['Char.Vitals', 'Char.State'],
    };
    const html = renderToStaticMarkup(
      <PromptPicker
        state={state}
        preview="now"
        env={NORD}
        cellW={7.8}
        refresh={0}
        onInsert={() => {}}
        onInsertLayout={() => {}}
      />,
    );
    expect(html).toContain('placeholder="Search values"');
    const groups = [...html.matchAll(/class="pc-picker-group">([^<]*)</g)].map((m) => m[1]);
    expect(groups).toEqual(['Vitals', 'Fight', 'Text and layout']);
    const rows = [
      ...html.matchAll(
        /class="pc-picker-name">([^<]*)<\/span><span class="pc-picker-value">([^<]*)</g,
      ),
    ].map((m) => [m[1], m[2]]);
    expect(rows.slice(0, 3)).toEqual([
      ['Health', '1020'],
      ['Opponent', 'in a fight'],
      ['Position', 'standing'],
    ]);
    expect(html).toMatch(/aria-selected="true"[^>]*class="pc-picker-row is-on is-current"/);
    expect(html).toContain('class="pc-picker-title">Health<');
    expect(html).toContain('From your prompt, and from the game when your prompt leaves it out.');
  });
});

describe('Edit as text', () => {
  it('offers the token rows, the help and Insert value… (P9)', () => {
    const html = renderToStaticMarkup(
      <PromptText
        template="%hp"
        tokens={[]}
        describedFor="%hp"
        onChange={() => {}}
        onCaretPiece={() => {}}
        onInsertValue={() => {}}
        insertRef={{ current: null }}
        caretRef={{ current: null }}
      />,
    );
    expect(html).toContain('contenteditable="true"');
    expect(html).toContain('aria-labelledby="pc-template-label"');
    expect(html).toContain(TEXT_HELP);
    const labels = [...html.matchAll(/class="pc-text-row-label">([^<]*)</g)].map((m) => m[1]);
    expect(labels).toEqual(['Forms', 'Color', 'Style', 'Layout']);
    expect(html).toContain('>%{if:fight}<');
    expect(html).toContain('Insert value…');
  });
});

describe('the D6 row', () => {
  it('names each Line trigger with Move to Prompts, none for a preset', () => {
    const html = renderToStaticMarkup(
      <LineTriggers
        triggers={[
          { name: 'Sleep when mana is low', pattern: '\\[\\d+/\\d+hp', preset: false },
          { name: 'From a preset', pattern: 'mv\\]', preset: true },
        ]}
        onMove={() => Promise.resolve()}
      />,
    );
    expect(html).toContain(
      'These triggers matched your prompt as a line. Vosh now sends your prompt only to Prompts triggers.',
    );
    expect(html).toContain('class="pc-d6-name">Sleep when mana is low<');
    expect(html).toContain('class="pc-d6-pattern">\\[\\d+/\\d+hp<');
    expect(html.match(/Move to Prompts/g)).toHaveLength(1);
    expect(
      renderToStaticMarkup(<LineTriggers triggers={[]} onMove={() => Promise.resolve()} />),
    ).toBe('');
  });
});

describe('the card at rest', () => {
  const rest = (promptsOff: boolean, note: string | null = null) =>
    renderToStaticMarkup(
      <Starts
        mode="rest"
        config={{
          draw: true,
          template: '%hp',
          previous_templates: [],
          capture: { kind: 'none' },
          show: 'text',
        }}
        presets={[]}
        designs={[]}
        values="live"
        refresh={0}
        env={NORD}
        cellW={7.8}
        onPick={() => {}}
        onInsertValue={() => {}}
        note={note}
        promptsOff={promptsOff}
      />,
    );

  it('says you turned prompts off in place of its hint (P14)', () => {
    const off = rest(true);
    expect(off).toContain('class="pc-hint is-warn" role="status"');
    expect(off).toContain(
      'You turned prompts off in the game. Type prompt in the game to turn them back on.',
    );
    expect(off).not.toContain('Click any part of your prompt');
    expect(rest(false)).toContain('Click any part of your prompt to change it.');
  });

  it('says what the Lament preview hides under its hint (P8c)', () => {
    const html = rest(false, 'Lament hides your vitals.');
    expect(html).toContain('class="pc-rest-note">Lament hides your vitals.<');
  });
});
