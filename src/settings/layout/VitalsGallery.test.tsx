import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, VITALS_STYLES, type UiConfig } from '../../ipc/uiConfig';
import { nextVitals, type Vitals } from '../../stores/gmcp/vitalsStore';
import type { BandEnv } from '../../terminal/bandCells';
import { VitalsTiles } from './VitalsGallery';
import { arrowPick, galleryCaption, galleryVitals, SAMPLE_VITALS, tileFit } from './vitalsStyles';
import { vitalsStylePick } from '../../panel/vitalsView';

// The Style gallery under Settings, Layout, Vitals (board 2 and board 5
// of the Vitals Styles review). The tiles draw from plain values here.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const config = (patch: Partial<UiConfig> = {}): UiConfig => ({
  ...normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: 'Menlo',
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  }),
  ...patch,
});

const ENV: BandEnv = {
  palette: Array.from({ length: 16 }, () => '#808080'),
  fg: '#cccccc',
  bg: '#101218',
  selection: '#333333',
  selectionText: '#ffffff',
  renderer: 'xterm',
  brightBold: false,
};

function draw(patch: Partial<UiConfig> = {}, vitals: Vitals = SAMPLE_VITALS): string {
  return renderToStaticMarkup(
    <VitalsTiles
      config={config(patch)}
      vitals={vitals}
      history={[]}
      text={null}
      env={ENV}
      inks={{}}
      flame="#eeca71"
      panel={300}
      width={300}
      scale={0.98}
      size={12}
      family={null}
      measure={(text) => text.length * 7}
      measureGame={(text) => text.length * 7}
      onPick={() => undefined}
    />,
  );
}

/** Each radio's style and whether it is checked, in order. */
const radios = (html: string) =>
  [...html.matchAll(/<input type="radio"([^>]*)\/>/g)].map(([, attrs]) => {
    const value = /value="([^"]*)"/.exec(attrs)?.[1];
    return `${value}${attrs.includes('checked=""') ? ' checked' : ''}`;
  });

/** Each tile's name and its mark, in order. */
const names = (html: string) =>
  [...html.matchAll(/<span class="st-vitals-name">(.*?)<\/span><\/label>/g)].map((m) =>
    m[1].replace(/<[^>]+>/g, ' ').trim(),
  );

describe('the Style gallery', () => {
  it('draws a tile for each style in the board order, named under each', () => {
    const html = draw();
    expect(radios(html).map((r) => r.split(' ')[0])).toEqual([...VITALS_STYLES]);
    expect(names(html)).toEqual([
      'Rows',
      'One line',
      'Ledger',
      'Gauges',
      'Pips',
      'Bands',
      'Ladders',
      'Blocks',
      'Traces',
      'Dials',
      'Rings',
      'Vials',
      'Orbs',
      'Candles',
      'Text',
    ]);
    expect(html).toContain('data-st-anchor="style"');
    expect(html).toContain('<legend class="visually-hidden">Style</legend>');
  });

  it('checks the style you play, your density until you pick one', () => {
    expect(radios(draw())).toContain('rows checked');
    expect(radios(draw({ vitals_density: 'line' }))).toContain('line checked');
    expect(radios(draw({ vitals_density: 'line', vitals_style: 'gauges' }))).toEqual([
      'rows',
      'line',
      'ledger',
      'gauges checked',
      'pips',
      'bands',
      'ladders',
      'blocks',
      'traces',
      'dials',
      'rings',
      'vials',
      'orbs',
      'candles',
      'text',
    ]);
  });

  it('marks the style your 0.7 vitals grew into, and none when they give no clue', () => {
    const html = draw({ vitals_style: 'text', vitals_legacy_style: 'text' });
    expect(names(html)).toEqual([
      'Rows',
      'One line',
      'Ledger',
      'Gauges',
      'Pips',
      'Bands',
      'Ladders',
      'Blocks',
      'Traces',
      'Dials',
      'Rings',
      'Vials',
      'Orbs',
      'Candles',
      'Text Yours in 0.7',
    ]);
    expect(radios(html)).toContain('text checked');
    expect(names(draw({ vitals_legacy_style: 'pips' }))[4]).toBe('Pips Yours in 0.7');
    expect(draw()).not.toContain('Yours in 0.7');
  });

  it('draws each tile at the panel width and scale, in your panel size, with no opponent', () => {
    const html = draw();
    expect(html).toContain('width:300px;transform:scale(0.98);--panel-text-px:12');
    expect(html).not.toContain('Opponent');
    expect(html).not.toContain('panel-vitals-row-combat');
  });

  it('draws the samples offline, 1020, 800 and 930', () => {
    const html = draw();
    expect(html).toContain('1020 / 1020');
    expect(html).toContain('800 / 800');
    expect(html).toContain('930 / 930');
  });

  it('writes the caption for the style you play and the width the tiles draw', () => {
    const html = draw({ vitals_style: 'gauges' });
    expect(html).toContain(
      'Gauges. Each vital fills a pill between its label and its value, as the Group pane shows your group. Each tile draws your vitals at your panel&#x27;s width, 300 pt, scaled to fit.',
    );
  });
});

describe('a pick', () => {
  it('writes Rows and One line to the density and clears the style', () => {
    expect(vitalsStylePick('rows')).toEqual({ vitals_density: 'rows', vitals_style: null });
    expect(vitalsStylePick('line')).toEqual({ vitals_density: 'line', vitals_style: null });
  });

  it('writes every other style alone and leaves the density as it is', () => {
    for (const style of ['ledger', 'gauges', 'pips', 'text'] as const) {
      expect(vitalsStylePick(style)).toEqual({ vitals_style: style });
    }
  });
});

describe('the arrow keys', () => {
  it('move the pick forward with Right and Down and back with Left and Up', () => {
    expect(arrowPick('ArrowRight', 'gauges')).toBe('pips');
    expect(arrowPick('ArrowDown', 'rows')).toBe('line');
    expect(arrowPick('ArrowLeft', 'gauges')).toBe('ledger');
    expect(arrowPick('ArrowUp', 'line')).toBe('rows');
  });

  it('go around the ends, as in any radio group', () => {
    expect(arrowPick('ArrowRight', 'text')).toBe('rows');
    expect(arrowPick('ArrowLeft', 'rows')).toBe('text');
  });

  it('leave every other key alone', () => {
    expect(arrowPick('Tab', 'rows')).toBeNull();
    expect(arrowPick('Enter', 'rows')).toBeNull();
  });
});

describe('the numbers the tiles draw', () => {
  it('are the catalog samples while no session has your vitals', () => {
    expect(galleryVitals(null)).toEqual({ vitals: SAMPLE_VITALS, history: [], live: false });
    expect(galleryVitals({ vitals: null, combat: null }).live).toBe(false);
    expect(SAMPLE_VITALS).toMatchObject({
      hp: 1020,
      maxhp: 1020,
      mana: 800,
      maxmana: 800,
      move: 930,
      maxmove: 930,
      hidden: false,
    });
  });

  it('are your own once the snapshot answers, low and hidden as the panel reads them', () => {
    const low = galleryVitals({
      vitals: {
        hp: '159',
        maxhp: '1020',
        mana: '310',
        maxmana: '800',
        move: '489',
        maxmove: '930',
      },
      combat: null,
    });
    expect(low.live).toBe(true);
    expect(low.vitals).toEqual(
      nextVitals(null, { hp: 159, maxhp: 1020, mana: 310, maxmana: 800, move: 489, maxmove: 930 }),
    );
    expect(low.vitals.low.hp).toBe(true);
    const hidden = galleryVitals({
      vitals: { hp: 0, maxhp: 0, mana: 0, maxmana: 0, move: 0, maxmove: 0, hidden: true },
      combat: null,
    });
    expect(hidden.vitals.hidden).toBe(true);
  });
});

describe('the tile width', () => {
  it('draws at your panel width and scales it to the tile', () => {
    expect(tileFit(300, 294)).toEqual({ width: 300, scale: 0.98 });
    expect(tileFit(300, 264)).toEqual({ width: 300, scale: 0.88 });
    expect(tileFit(300, 320)).toEqual({ width: 300, scale: 1 });
  });

  it('never scales under three quarters, and draws a wide panel at the width that allows', () => {
    expect(tileFit(420, 264)).toEqual({ width: 352, scale: 0.75 });
    expect(galleryCaption('rows', 420, 352)).toBe(
      'Rows. Each vital gets a row of its own, its value at the right and a line under it. Your panel is 420 pt wide, so each tile draws your vitals at 352 pt, scaled to fit.',
    );
  });

  it('says Ladders leaves the last peak lit after a hit', () => {
    expect(galleryCaption('ladders', 300, 300)).toMatch(
      /^Ladders\. Each vital lights a row of segments, and a hit leaves its last peak lit for a moment\. /,
    );
  });

  it('draws at the panel width before the tile is measured', () => {
    expect(tileFit(300, 0)).toEqual({ width: 300, scale: 1 });
  });
});
