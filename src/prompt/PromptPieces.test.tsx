import { isValidElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { BandEnv } from '../terminal/bandCells';
import { BY_VALUE_HINT, THEME_HINT, WHEN_FIXED_HINT } from './promptPieces';
import { TEXT_HELP } from './textEdit';
import type { PromptFieldState, PromptPreset, PromptState } from '../ipc/prompt';
import type { PromptForm, PromptPiece } from '../ipc/promptDesign';
import { LineTriggers } from './PromptCodes';
import { PromptPicker } from './PromptPicker';
import { MoreStyleItems, PromptPieceBody } from './PromptPiece';
import { Starts } from './PromptStarts';
import { PromptText } from './PromptText';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

type ItemProps = {
  role?: string;
  onClick?: () => void;
  onPointerMove?: (e: { currentTarget: unknown }) => void;
  children?: ReactNode;
};

/** The props of the More styles item `label` among what MoreStyleItems
 *  returns, since nothing here renders it. */
function findItem(tree: ReactNode, label: string): ItemProps {
  type Props = ItemProps;
  const items: Props[] = [];
  const walk = (node: ReactNode) => {
    if (Array.isArray(node)) node.forEach(walk);
    else if (isValidElement<Props>(node)) {
      if (node.props.role?.startsWith('menuitem')) items.push(node.props);
      walk(node.props.children);
    }
  };
  walk(tree);
  const text = (node: ReactNode): string => {
    if (typeof node === 'string') return node;
    if (Array.isArray(node)) return node.map(text).join('');
    if (isValidElement<Props>(node) && typeof node.type === 'string') {
      return text(node.props.children);
    }
    return '';
  };
  const item = items.find((props) => text(props.children) === label);
  if (!item) throw new Error(`no item ${label}`);
  return item;
}

/** Press the More styles item `label` through its click handler. */
function pressItem(tree: ReactNode, label: string) {
  const item = findItem(tree, label);
  if (!item.onClick) throw new Error(`item ${label} takes no click`);
  item.onClick();
}

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
  selection: '#4c566a',
  selectionText: '#eceff4',
  renderer: 'xterm',
  brightBold: false,
};

const form = (format: PromptForm['format'], segment: string, label = segment): PromptForm => ({
  format,
  label,
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
  background: { kind: 'default' },
  bold: false,
  dim: false,
  italic: true,
  underline: false,
  underline_style: null,
  underline_color: { kind: 'default' },
  inverse: false,
  strike: false,
  blink: false,
  literal: null,
  meta: '1020 of 1020',
  forms: [
    form('value', '1020', 'Current'),
    form('cur_max', '1020/1020', 'Current and max'),
    form('percent', '100%', 'Percent'),
    form('bar', 'Bar'),
  ],
  by_value: true,
  shows: true,
};

const draw = (piece: PromptPiece) =>
  renderToStaticMarkup(
    <PromptPieceBody piece={piece} env={NORD} onEdit={() => {}} onInsertValue={() => {}} />,
  );

/** The markup of the row `label`, up to the row after it. */
function row(html: string, label: string): string {
  const start = html.indexOf(`class="pc-piece-row" role="group" aria-label="${label}"`);
  if (start < 0) return '';
  const next = html.indexOf('class="pc-piece-row', start + 1);
  return html.slice(start, next < 0 ? undefined : next);
}

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
    // Each segment's name holds the text it shows, then the form's name.
    expect(html).toContain('aria-label="1020/1020, Current and max"');
    expect(html).not.toMatch(/aria-label="Current and max"/);
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
    const swatches = [
      ...row(html, 'Color').matchAll(/aria-label="(Theme [a-z]+)"[^>]*background:([^"]*)"/g),
    ].map((m) => [m[1], m[2]]);
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

  it('gives every part that takes a color a Background, under Color (styles board)', () => {
    const html = draw(HP);
    const ground = row(html, 'Background');
    // The hint stays right under Color, as P5 draws it.
    expect(html.indexOf(THEME_HINT)).toBeGreaterThan(html.indexOf('aria-label="Color"'));
    expect(html.indexOf('aria-label="Background"')).toBeGreaterThan(html.indexOf(THEME_HINT));
    expect(ground).toMatch(/aria-label="Terminal background"[^>]*aria-pressed="true"/);
    expect(ground).toContain('background:#2e3440');
    const swatches = [...ground.matchAll(/aria-label="(Theme [a-z]+)"/g)].map((m) => m[1]);
    expect(swatches).toHaveLength(7);
    expect(ground).toContain('aria-label="Custom background"');
    expect(ground).toContain('placeholder="Custom"');
    // A true color fills Custom with its hex and rings it.
    const rgb = draw({ ...HP, background: { kind: 'rgb', r: 0x3b, g: 0x42, b: 0x52 } });
    expect(row(rgb, 'Background')).toMatch(/class="st-color pc-custom is-on"/);
    expect(row(rgb, 'Background')).toContain('value="#3b4252"');
    // A theme color presses its swatch.
    const blue = row(draw({ ...HP, background: { kind: 'named', index: 4 } }), 'Background');
    expect(blue).toMatch(/aria-label="Theme blue"[^>]*aria-pressed="true"/);
  });

  it('offers By value on the ground of a value, where the Color row has it', () => {
    const ground = row(draw(HP), 'Background');
    expect(ground).toMatch(/aria-label="By value"[^>]*aria-pressed="false"/);
    expect(ground.indexOf('aria-label="By value"')).toBeLessThan(ground.indexOf('Theme red'));
    // A ground by value presses it, and the hint gives its rule.
    const html = draw({ ...HP, background: { kind: 'by_value' } });
    expect(row(html, 'Background')).toMatch(/aria-label="By value"[^>]*aria-pressed="true"/);
    expect(row(html, 'Background')).toContain('class="st-color pc-custom"');
    expect(html).toContain(BY_VALUE_HINT.replace(/'/g, '&#x27;'));
    // Text has no By value, so its ground has none either.
    expect(row(draw({ ...HP, by_value: false }), 'Background')).not.toContain('By value');
  });

  it('names a color by value in the field when no swatch shows it', () => {
    const named = (html: string, label: string, field: string) => {
      const part = row(html, label);
      const at = part.indexOf(`aria-label="${field}"`);
      return part.slice(part.lastIndexOf('<span class="st-color', at), at);
    };
    // The text by Mana on Health, and the ground by the game's colors.
    const html = draw({
      ...HP,
      color: { kind: 'by_value', field: 'mana' },
      background: { kind: 'by_value', game: true },
    });
    const color = row(html, 'Color');
    expect(color).toContain('placeholder="By mana"');
    expect(color).toContain('class="st-color pc-custom is-on"');
    expect(named(html, 'Color', 'Custom color')).toContain(
      'background:linear-gradient(90deg, #a3be8c 0 33.333%, #ebcb8b 0 66.667%, #bf616a 0)',
    );
    expect(row(html, 'Background')).toContain('placeholder="By game"');
    expect(html).toContain(BY_VALUE_HINT.replace(/'/g, '&#x27;'));
    expect(row(html, 'Background')).toContain('class="st-color pc-custom is-on"');
    // A color the field writes keeps its plain placeholder.
    expect(row(draw(HP), 'Color')).toContain('placeholder="Custom"');
  });

  it('shows an underline colored by value as By value, its line in full color', () => {
    const line = row(
      draw({
        ...HP,
        underline: true,
        underline_style: 'curly',
        underline_color: { kind: 'by_value' },
      }),
      'Underline',
    );
    expect(line).toContain('placeholder="By value"');
    expect(line).toContain('class="st-color pc-custom is-on"');
    expect(line).toContain('#a3be8c 0 33.333%');
    expect(line).not.toContain('placeholder="Text color"');
    expect(line).toMatch(/text-decoration-style:wavy;text-decoration-color:#a3be8c">Curly</);
  });

  it('keeps B, I and U and adds More styles after them', () => {
    const style = row(draw(HP), 'Style');
    const order = ['aria-label="Bold"', 'aria-label="Italic"', 'aria-label="Underline"'].map((a) =>
      style.indexOf(a),
    );
    expect(order.every((i) => i >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    expect(style).toMatch(
      /class="pc-style-more" aria-haspopup="menu" aria-expanded="false"><span>More styles<\/span>/,
    );
    // The button reads the styles it holds that are on, pressed.
    const on = row(draw({ ...HP, strike: true, dim: true }), 'Style');
    expect(on).toContain('class="pc-style-more is-on"');
    expect(on).toContain('aria-label="More styles, Strikethrough, dim on"');
    expect(on).toContain('<span>Strikethrough, dim</span>');
    // An underline kind past the single line reads there too.
    const curly = row(draw({ ...HP, underline: true, underline_style: 'curly' }), 'Style');
    expect(curly).toContain('<span>Curly underline</span>');
    expect(curly).toContain('class="pc-style-more is-on"');
  });

  it('lists the styles past B, I and U, then the underline kinds, a check on each that is on', () => {
    const html = renderToStaticMarkup(
      <ul>
        <MoreStyleItems
          piece={{
            strike: true,
            dim: false,
            inverse: false,
            blink: false,
            underline_style: 'dotted',
          }}
          onToggle={() => undefined}
        />
      </ul>,
    );
    const items = [
      ...html.matchAll(
        /role="(menuitemcheckbox|menuitemradio)" aria-checked="(true|false)"[^>]*>(?:<svg[^]*?<\/svg>)?<span class="pc-style-sample is-([a-z]+)">([^<]*)</g,
      ),
    ].map((m) => [m[4], m[3], m[2], m[1]]);
    // The styles are checkboxes, and the kinds radios, one on at a time.
    expect(items).toEqual([
      ['Strikethrough', 'strike', 'true', 'menuitemcheckbox'],
      ['Dim', 'dim', 'false', 'menuitemcheckbox'],
      ['Reverse', 'inverse', 'false', 'menuitemcheckbox'],
      ['Blink', 'blink', 'false', 'menuitemcheckbox'],
      ['Double underline', 'double', 'false', 'menuitemradio'],
      ['Curly underline', 'curly', 'false', 'menuitemradio'],
      ['Dotted underline', 'dotted', 'true', 'menuitemradio'],
      ['Dashed underline', 'dashed', 'false', 'menuitemradio'],
    ]);
    expect(html.match(/pc-start-check/g)).toHaveLength(2);
    // A rule sets the underline kinds apart from the styles.
    expect(html.indexOf('role="separator"')).toBeGreaterThan(html.indexOf('>Blink<'));
    expect(html.indexOf('role="separator"')).toBeLessThan(html.indexOf('>Double underline<'));
  });

  // The pointer and the arrow keys share one highlight, so the row
  // under the pointer takes the focus.
  it('gives the row under the pointer the focus', () => {
    const doc: { activeElement: unknown } = { activeElement: null };
    vi.stubGlobal('document', doc);
    try {
      const piece = {
        strike: false,
        dim: false,
        inverse: false,
        blink: false,
        underline_style: null,
      };
      const row = {
        focus() {
          doc.activeElement = row;
        },
      };
      findItem(MoreStyleItems({ piece, onToggle: () => undefined }), 'Dim').onPointerMove?.({
        currentTarget: row,
      });
      expect(doc.activeElement).toBe(row);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('turns the underline on in a kind you pick, and off with the kind that is on', () => {
    const picks = (underline_style: PromptPiece['underline_style']) => {
      const toggled: [string, boolean][] = [];
      const piece = { strike: false, dim: false, inverse: false, blink: false, underline_style };
      for (const label of ['Blink', 'Curly underline', 'Dotted underline']) {
        pressItem(
          MoreStyleItems({ piece, onToggle: (style, on) => toggled.push([style, on]) }),
          label,
        );
      }
      return toggled;
    };
    // No underline: each kind turns it on in that kind.
    expect(picks(null)).toEqual([
      ['blink', true],
      ['curly', true],
      ['dotted', true],
    ]);
    // A dotted line: curly takes its place, and dotted again ends it.
    expect(picks('dotted')).toEqual([
      ['blink', true],
      ['curly', true],
      ['dotted', false],
    ]);
  });

  it('shows the Underline row while U is on, its kind and its color (styles board)', () => {
    expect(draw(HP)).not.toContain('aria-label="Underline color"');
    const html = draw({
      ...HP,
      underline: true,
      underline_style: 'curly',
      underline_color: { kind: 'rgb', r: 191, g: 97, b: 106 },
    });
    expect(html).toMatch(/aria-label="Underline" aria-pressed="true"/);
    const line = row(html, 'Underline');
    // Five kinds, each word in its own line and in the line's color.
    const kinds = [
      ...line.matchAll(
        /aria-pressed="(true|false)"><span class="pc-line" style="text-decoration-style:([a-z]+);text-decoration-color:#bf616a">([A-Za-z]+)</g,
      ),
    ].map((m) => [m[3], m[2], m[1]]);
    expect(kinds).toEqual([
      ['Single', 'solid', 'false'],
      ['Double', 'double', 'false'],
      ['Curly', 'wavy', 'true'],
      ['Dotted', 'dotted', 'false'],
      ['Dashed', 'dashed', 'false'],
    ]);
    expect(line).toContain('aria-label="Underline color"');
    expect(line).toContain('value="#bf616a"');
    expect(line).toContain('class="st-color pc-custom is-on"');
    // With the text's color, the field is empty and its swatch shows the
    // text color.
    const plain = row(draw({ ...HP, underline: true, underline_style: 'underline' }), 'Underline');
    expect(plain).toContain('placeholder="Text color"');
    expect(plain).toContain('value=""');
    expect(plain).toContain('background:#e5e9f0');
    expect(plain).toMatch(/aria-pressed="true"><span class="pc-line" style="[^"]*">Single</);
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
        session={1}
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
    // The search names the highlighted value as the one a reader is on,
    // since focus stays in the search while the arrows move the list.
    const search = /<input[^>]*aria-label="Search values"[^>]*>/.exec(html)?.[0] ?? '';
    expect(search).toContain('role="combobox"');
    expect(search).toContain('aria-expanded="true"');
    const active = /aria-activedescendant="([^"]+)"/.exec(search)?.[1];
    const controls = /aria-controls="([^"]+)"/.exec(search)?.[1];
    expect(active).toBeTruthy();
    expect(html).toMatch(
      new RegExp(`id="${controls}"[^>]*role="listbox"|role="listbox"[^>]*id="${controls}"`),
    );
    expect(html).toMatch(
      new RegExp(`id="${active}"[^>]*aria-selected="true"|aria-selected="true"[^>]*id="${active}"`),
    );
    expect(html).toContain(`id="${active}"`);
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

describe('the start list while your design follows the game', () => {
  const GAME = '[%{c:hp:game}%hp%c_reset/%{maxhp}hp] ';
  const presets: PromptPreset[] = [
    { id: 'default', label: "Vosh's default", template: 'DEFAULT ' },
    { id: 'game', label: 'Same as the game', template: GAME },
    { id: 'minimal', label: 'Minimal', template: '%{hp}h %{mana}m %{move}v > ' },
    { id: 'empty', label: 'Start empty', template: '' },
  ];
  const starts = (mirror: boolean, template: string) => (
    <Starts
      session={1}
      mode="start"
      config={{
        draw: false,
        template,
        previous_templates: [],
        capture: { kind: 'aabahran', prompt: '[%h/%Hhp] ', fprompt: '', follow_game: true },
        show: 'text',
        mirror,
      }}
      presets={presets}
      designs={[]}
      values="live"
      refresh={0}
      env={NORD}
      cellW={7.8}
      onPick={() => {}}
      onInsertValue={() => {}}
    />
  );
  /** The names of the rows that carry the check. */
  const checked = (html: string) =>
    [
      ...html.matchAll(
        /aria-checked="true"[^>]*>(?:<svg[\s\S]*?<\/svg>)?<span class="pc-start-name">([^<]*)</g,
      ),
    ].map((m) => m[1].replace(/&#x27;/g, "'"));

  it('checks Same as the game', () => {
    expect(checked(renderToStaticMarkup(starts(true, GAME)))).toEqual(['Same as the game']);
  });

  it('checks nothing while there are no codes to follow, not even Start empty', () => {
    expect(checked(renderToStaticMarkup(starts(true, '')))).toEqual([]);
  });

  it('checks the row that holds your own design, never Same as the game', () => {
    expect(checked(renderToStaticMarkup(starts(false, 'DEFAULT ')))).toEqual(["Vosh's default"]);
    expect(checked(renderToStaticMarkup(starts(false, GAME)))).toEqual([]);
    expect(checked(renderToStaticMarkup(starts(false, '')))).toEqual(['Start empty']);
  });
});

describe('the card at rest', () => {
  const rest = (
    promptsOff: boolean,
    note: string | null = null,
    notMatching: string | null = null,
  ) =>
    renderToStaticMarkup(
      <Starts
        session={1}
        mode="rest"
        config={{
          draw: true,
          template: '%hp',
          previous_templates: [],
          capture: { kind: 'none' },
          show: 'text',
          mirror: false,
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
        notMatching={notMatching}
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

  it('says no prompt matched in place of its hint, after prompts off', () => {
    const line =
      'No prompt has matched since 8:12. If you changed it in the game, point at it again.';
    const html = rest(false, null, line);
    expect(html).toContain('class="pc-hint is-warn" role="status"');
    expect(html).toContain(line);
    expect(html).not.toContain('Click any part of your prompt');
    expect(rest(true, null, line)).not.toContain(line);
  });

  it('says what the Lament preview hides under its hint (P8c)', () => {
    const html = rest(false, 'Lament hides your vitals.');
    expect(html).toContain('class="pc-rest-note">Lament hides your vitals.<');
  });
});
