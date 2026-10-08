import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PromptConfig } from '../ipc/prompt';
import { VOSH_VITALS_TEXT } from '../ipc/vitals';
import type { BandEnv } from '../terminal/bandCells';
import { VITALS_TEXT_BINDING, vitalsTable, vitalsTextSave } from './cardBinding';
import { besideAnchor, cardNames, VITALS_MORE, vitalsStartRows } from './cardRules';
import { TextFoot } from './PromptFoot';
import { Starts } from './PromptStarts';
import { VitalsTextBlock } from '../panel/VitalsText';
import { vitalsTextLines, type PieceCell } from '../panel/vitalsTextFit';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The card bound to your vitals text.

const ENV: BandEnv = {
  palette: Array.from({ length: 16 }, () => '#888888'),
  fg: '#e5e9f0',
  bg: '#2e3440',
  selection: '#4c566a',
  selectionText: '#eceff4',
  renderer: 'xterm',
  brightBold: false,
};

const MINE = '%hp/%{maxhp}hp %mana/%{maxmana}mn';
const BEFORE = '%hp %mana %move';
const LEGACY = '%hp/%maxhp %mana/%maxmn %move/%maxmv';

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  vi.mocked(emit).mockClear();
});

describe('the vitals text card', () => {
  it('is titled Your vitals text', () => {
    expect(cardNames('vitals')).toEqual({
      title: 'Your vitals text',
      options: 'Vitals text options',
    });
    // Your prompt's card keeps its own.
    expect(cardNames('prompt').title).toBe('Customize prompt');
  });

  it('leaves out Draw your prompt and where your prompt shows, and says where it draws', () => {
    const html = renderToStaticMarkup(
      <TextFoot
        note="Draws in your panel"
        preview="now"
        forsaken
        onPreview={() => {}}
        onDone={() => {}}
      />,
    );
    expect(html).toContain('<span class="pc-foot-note">Draws in your panel</span>');
    expect(html).toContain('Preview');
    expect(html).toContain('Done');
    expect(html).not.toContain('Draw your prompt');
    expect(html).not.toContain('In the text');
  });

  it('rests on the parts with Insert value and Presets, and no prompt warning', () => {
    const table = vitalsTable(MINE, [MINE], null);
    const html = renderToStaticMarkup(
      <Starts
        session={1}
        mode="rest"
        config={table}
        presets={[]}
        designs={[]}
        own={vitalsStartRows(table, VOSH_VITALS_TEXT, null)}
        values="live"
        refresh={0}
        env={ENV}
        cellW={7.8}
        restHint="Click any part of your vitals to change it."
        onPick={() => {}}
        onInsertValue={() => {}}
      />,
    );
    expect(html).toContain('Click any part of your vitals to change it.');
    expect(html).toContain('Insert value…');
    expect(html).toContain('Presets');
    expect(html).not.toContain('prompts off');
  });

  it('offers Customize vitals in More, and nothing about a game prompt', () => {
    expect(VITALS_MORE).toEqual([{ id: 'customize-vitals', label: 'Customize vitals…' }]);
  });
});

describe('the vitals text Presets', () => {
  const rows = (table: PromptConfig, legacy: string | null) =>
    vitalsStartRows(table, VOSH_VITALS_TEXT, legacy).rows.map((r) => [r.label, r.checked]);

  it("lists Vosh's text, yours, the one before it and your 0.7 text, yours checked", () => {
    const list = vitalsStartRows(vitalsTable(MINE, [MINE, BEFORE], null), VOSH_VITALS_TEXT, LEGACY);
    expect(list.rows.map((r) => [r.label, r.template, r.checked])).toEqual([
      ["Vosh's text", VOSH_VITALS_TEXT, false],
      ['Yours', MINE, true],
      ['Your text before that', BEFORE, false],
      ['Your 0.7 text', LEGACY, false],
    ]);
    // No prompt presets, no other profile, no Start empty.
    expect(list.others).toEqual([]);
    expect(list.empty).toBeNull();
  });

  it("checks Vosh's text while you have none, and leaves out what is not there", () => {
    expect(rows(vitalsTable('', [], null), null)).toEqual([["Vosh's text", true]]);
  });

  it('checks your 0.7 text while you have none and it was on', () => {
    expect(rows(vitalsTable('', [], LEGACY), LEGACY)).toEqual([
      ["Vosh's text", false],
      ['Your 0.7 text', true],
    ]);
  });

  it('lists a text once, under the first row that holds it', () => {
    expect(rows(vitalsTable(MINE, [MINE, VOSH_VITALS_TEXT], null), MINE)).toEqual([
      ["Vosh's text", false],
      ['Yours', true],
    ]);
  });
});

describe('what the vitals text card saves', () => {
  it('opens with the text you had first among the earlier ones', async () => {
    vi.mocked(invoke).mockImplementation((cmd) =>
      Promise.resolve(
        cmd === 'ui_get_config' ? { vitals_text: MINE, vitals_text_previous: [BEFORE] } : null,
      ),
    );
    const table = await VITALS_TEXT_BINDING.open(1);
    expect(table.template).toBe(MINE);
    expect(table.previous_templates).toEqual([MINE, BEFORE]);
    expect(table.draw).toBe(true);
    expect(table.capture).toEqual({ kind: 'none' });
  });

  it('writes vitals_text through its setter, and keeps the text you opened with as Yours', async () => {
    vi.mocked(invoke).mockImplementation((cmd) =>
      Promise.resolve(cmd === 'ui_get_config' ? {} : null),
    );
    const edited = { ...vitalsTable(MINE, [MINE, BEFORE], null), template: `${MINE} %move` };
    await VITALS_TEXT_BINDING.write(edited, { asIs: false, session: 1 });
    expect(invoke).toHaveBeenCalledWith('ui_set_fields', {
      fields: [
        { field: 'vitals_text', value: `${MINE} %move` },
        { field: 'vitals_text_previous', value: [MINE, BEFORE] },
      ],
      profile: null,
    });
    // And tells every window, so Customize vitals shows it.
    expect(emit).toHaveBeenCalledWith('vosh://vitals-text-changed', {
      vitals_text: `${MINE} %move`,
      vitals_text_previous: [MINE, BEFORE],
    });
  });

  it("saves Vosh's text as none, so it follows Vosh's", () => {
    expect(vitalsTextSave(vitalsTable('', [MINE], null), null).vitals_text).toBe('');
  });

  it('starts on your 0.7 template while it was on, and saves that as none', async () => {
    // A profile whose 0.7 template was on starts its Text there.
    const table = vitalsTable('', [], LEGACY);
    expect(table.template).toBe(LEGACY);
    expect(vitalsTextSave(table, LEGACY).vitals_text).toBe('');
    // Picking Vosh's text then keeps it, rather than falling back to 0.7.
    const vosh = { ...table, template: VOSH_VITALS_TEXT };
    expect(vitalsTextSave(vosh, LEGACY).vitals_text).toBe(VOSH_VITALS_TEXT);
    vi.mocked(invoke).mockImplementation((cmd) =>
      Promise.resolve(
        cmd === 'ui_get_config'
          ? { vitals_text: '', vitals_text_previous: [], vitals_legacy_text: LEGACY }
          : null,
      ),
    );
    expect((await VITALS_TEXT_BINDING.open(1)).template).toBe(LEGACY);
  });
});

describe('where the vitals text card sits', () => {
  it('floats 12 px from the panel with its foot over the input band, as board 5 draws it', () => {
    // A 1280 by 800 window with a 300 pt panel, a 40 px input band and a
    // 28 px status line under the terminal.
    expect(
      besideAnchor({
        areaTop: 32,
        areaRight: 980,
        areaBottom: 732,
        viewportW: 1280,
        viewportH: 800,
      }),
    ).toEqual({ right: 312, bottom: 84, maxHeight: 676 });
  });
});

describe('the footer while the vitals text card is open', () => {
  const plain = { kind: 'default' } as const;
  const span = (piece: number, col: number, width: number) => ({
    piece,
    row: 0,
    col,
    width,
    fg: plain,
    bg: plain,
    bold: false,
    italic: false,
    underline: false,
  });
  // 159/1020hp, three parts: the value, the slash and max, the letters.
  const rendered = {
    ansi: '159/1020hp',
    plain: '159/1020hp',
    rows: 1,
    spans: [span(1, 0, 3), span(2, 3, 5), span(3, 8, 2)],
  };
  const text = { session: 1, live: rendered, full: rendered, fight: [false], right: [] };

  it('marks each cell with the part that drew it', () => {
    const [line] = vitalsTextLines(text, 30, false, true);
    expect((line.left as PieceCell[]).map((cell) => cell.piece)).toEqual([
      1, 1, 1, 2, 2, 2, 2, 2, 3, 3,
    ]);
    // Closed, the cells carry no part.
    const [closed] = vitalsTextLines(text, 30, false);
    expect((closed.left as PieceCell[]).some((cell) => cell.piece !== undefined)).toBe(false);
  });

  it('rings the part you picked and lets every part take a click', () => {
    const pick = vi.fn();
    const html = renderToStaticMarkup(
      <VitalsTextBlock
        lines={vitalsTextLines(text, 30, false, true)}
        env={ENV}
        marks={{ template: MINE, picked: 1, preview: 'now', pick }}
      />,
    );
    expect(html.match(/class="panel-vitals-text-part[^"]*"/g)).toEqual([
      'class="panel-vitals-text-part is-picked"',
      'class="panel-vitals-text-part"',
      'class="panel-vitals-text-part"',
    ]);
    // Closed, the footer draws its runs alone.
    const closed = renderToStaticMarkup(
      <VitalsTextBlock lines={vitalsTextLines(text, 30, false)} env={ENV} />,
    );
    expect(closed).not.toContain('panel-vitals-text-part');
  });
});
