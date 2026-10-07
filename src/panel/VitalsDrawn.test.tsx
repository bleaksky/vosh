import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfig';
import type { CombatOpponent, Fight } from '../stores/gmcp/combatStore';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import { PaneTextSizeContext } from './paneTextSize';
import { VitalsBlock, type VitalsBlockProps } from './VitalsFooter';
import { bandsHeight } from './vitalsDrawnFit';
import { marksHeight } from './vitalsMarksFit';
import type { VitalsFit } from './vitalsFit';
import type { HitViews } from './vitalsHit';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The styles of the More Vitals Styles review, with the numbers its
// boards draw: Tolliver at 744 of 1038, 590 of 870 and 402 of 521 a
// moment after a hit, in a fight with a Blackwatch guard at 54 percent
// that began with him at 905, 870 and 402 and the guard at 100.
const HIT: Vitals = {
  hp: 744,
  maxhp: 1038,
  mana: 590,
  maxmana: 870,
  move: 402,
  maxmove: 521,
  low: { hp: false, mana: false, move: false },
  hidden: false,
};
const GUARD: CombatOpponent = {
  name: 'a Blackwatch guard',
  hp_pct: 54,
  condition: null,
  hidden: false,
  tank: null,
};
const FIGHT: Fight = {
  name: 'a Blackwatch guard',
  start: { hp: 905, maxhp: 1038, mana: 870, maxmana: 870, move: 402, maxmove: 521 },
  healths: [100, 61, 54],
};

function draw(
  fit: VitalsFit,
  options: Partial<VitalsOptions> = {},
  props: Partial<VitalsBlockProps> = {},
): string {
  return renderToStaticMarkup(
    <PaneTextSizeContext.Provider value={12}>
      <VitalsBlock
        vitals={HIT}
        combat={GUARD}
        fight={FIGHT}
        fit={fit}
        options={{ ...DEFAULT_VITALS_OPTIONS, ...options }}
        {...props}
      />
    </PaneTextSizeContext.Provider>,
  );
}

/** Every match of `re`'s first group in `html`. */
const all = (html: string, re: RegExp) => [...html.matchAll(re)].map((m) => m[1]);

describe('Bands', () => {
  const BANDS: VitalsFit = { style: 'bands' };

  it('draws a bar over the quiet bands under each label and value', () => {
    const html = draw(BANDS);
    expect(all(html, /vitals-band-bar" style="width:([\d.]+)%/g).map(Number)).toEqual([
      54,
      (744 / 1038) * 100,
      (590 / 870) * 100,
      (402 / 521) * 100,
    ]);
    expect(all(html, /vitals-band-zone (is-\w+)/g)).toEqual(
      Array.from({ length: 4 }, () => ['is-low', 'is-mid']).flat(),
    );
    expect(all(html, /vitals-mark-label">([^<]+)/g)).toEqual([
      'a Blackwatch guard',
      'Health',
      'Mana',
      'Moves',
    ]);
  });

  it('stands a tick where each vital and your opponent began the fight', () => {
    const html = draw(BANDS);
    expect(all(html, /class="(vitals-band-tick[^"]*)" style="left:([\d.]+)%/g)).toHaveLength(4);
    expect(
      [...html.matchAll(/class="(vitals-band-tick[^"]*)" style="left:([\d.]+)%/g)].map((m) => [
        m[1],
        Number(m[2]),
      ]),
    ).toEqual([
      ['vitals-band-tick is-end', 100],
      ['vitals-band-tick', (905 / 1038) * 100],
      ['vitals-band-tick is-end', 100],
      ['vitals-band-tick', (402 / 521) * 100],
    ]);
  });

  it('draws no tick out of a fight, and your opponent where you put it', () => {
    expect(draw(BANDS, {}, { combat: null })).not.toContain('vitals-band-tick');
    const html = draw(BANDS, { opponent: 'bottom' });
    expect(html).toContain('vitals-foe vitals-tone is-foe is-bottom');
    expect(html.indexOf('Moves')).toBeLessThan(html.indexOf('a Blackwatch guard'));
    expect(draw(BANDS, {}, { vitals: null, opponentOnly: true })).toContain('is-alone');
  });

  it('holds the height of your vitals while it waits for them', () => {
    expect(bandsHeight(12, 3)).toBe(1 + 9 + 3 * 27 + 2 * 5 + 11);
    expect(bandsHeight(16, 1)).toBe(1 + 12 + (21 + 4 + 8) + 15);
    const html = draw(BANDS, {}, { vitals: null, combat: null });
    expect(html).toContain('Vitals appear when you log in.');
    expect(html).toContain(`--vitals-min-height:${bandsHeight(12, 3)}px`);
  });
});

describe('Ladders', () => {
  const BESIDE: VitalsFit = { style: 'ladders', fit: 'beside' };
  const UNDER: VitalsFit = { style: 'ladders', fit: 'under' };

  /** The lit and unlit segments of each ladder, in order. */
  const ladders = (html: string) =>
    all(html, /<span class="vitals-ladder">(.*?)<\/span>/g).map((segs) => [
      (segs.match(/is-lit/g) ?? []).length,
      (segs.match(/<i /g) ?? []).length,
    ]);

  it('lights each vital its share of 24 segments, and your opponent of 48', () => {
    const html = draw(BESIDE);
    expect(ladders(html)).toEqual([
      [26, 48],
      [17, 24],
      [16, 24],
      [19, 24],
    ]);
    expect(html).toContain('vitals-marks is-ladders"');
    expect(all(html, /vitals-mark-label">([^<]+)/g)).toEqual([
      'a Blackwatch guard',
      'Health',
      'Mana',
      'Moves',
    ]);
  });

  it('drops each ladder under its label and value at 200 pt', () => {
    const html = draw(UNDER);
    expect(html).toContain('panel-vitals panel-vitals-marks is-under');
    expect(html).toContain('vitals-marks is-ladders is-under');
  });

  it('lights none for a value the game hides', () => {
    const html = draw(BESIDE, {}, { vitals: { ...HIT, hidden: true } });
    expect(ladders(html).slice(1)).toEqual([
      [0, 24],
      [0, 24],
      [0, 24],
    ]);
  });

  it('holds the height of your vitals while it waits for them', () => {
    const html = draw(BESIDE, {}, { vitals: null, combat: null });
    expect(html).toContain('Vitals appear when you log in.');
    expect(html).toContain(`--vitals-min-height:${marksHeight(12, 3)}px`);
  });
});

describe('Show each hit', () => {
  // Board 4: the guard went from 61 to 54 and Tolliver from 851 to 744.
  const HEALTH_WAS = (851 / 1038) * 100;
  const HEALTH = (744 / 1038) * 100;
  const HITS: HitViews = {
    hp: { fill: HEALTH, ghost: HEALTH_WAS, draining: false, peak: HEALTH_WAS },
    foe: { fill: 54, ghost: 61, draining: false, peak: 61 },
  };
  const gone = (html: string, name: string) =>
    [
      ...html.matchAll(
        new RegExp(`${name} vitals-ghost" style="left:([\\d.]+)%;width:([\\d.]+)%`, 'g'),
      ),
    ].map((m) => [Number(m[1]), Number(m[2])]);

  it('leaves the part a hit took pale beside the Rows meter', () => {
    const html = draw({ style: 'rows' }, {}, { hits: HITS });
    expect(gone(html, 'panel-vitals-gone')).toEqual([
      [54, 7],
      [HEALTH, HEALTH_WAS - HEALTH],
    ]);
  });

  it('leaves it on the Ledger line, the Gauges pill and the Bands bar', () => {
    expect(
      gone(draw({ style: 'ledger', fit: 'full' }, {}, { hits: HITS }), 'vitals-ledger-gone'),
    ).toHaveLength(2);
    const gauges = draw({ style: 'gauges', fit: 'beside' }, {}, { hits: HITS });
    expect(gone(gauges, 'vitals-gauge-gone')).toEqual([[HEALTH, HEALTH_WAS - HEALTH]]);
    expect(gauges).toContain('vitals-gauge-fill is-hit');
    expect(gone(draw({ style: 'bands' }, {}, { hits: HITS }), 'vitals-band-gone')).toHaveLength(2);
  });

  it('leaves the discs a hit put out pale on Pips', () => {
    const html = draw({ style: 'pips', fit: 'ten' }, {}, { hits: HITS });
    // 72 percent lights seven discs, 82 lit eight.
    const health = all(html, /<span class="vitals-pips"[^>]*>(.*?)<\/span><\/span>/g)[0] ?? '';
    expect(all(health, /class="vitals-pip([^"]*)"/g)).toEqual([
      ...Array(7).fill(' is-full'),
      ' is-gone',
      '',
      '',
    ]);
  });

  it('holds the Ladders peak lit, one segment over the fill', () => {
    const html = draw({ style: 'ladders', fit: 'beside' }, {}, { hits: HITS });
    const health = all(html, /<span class="vitals-ladder">(.*?)<\/span>/g)[1] ?? '';
    const lit = all(health, /class="vitals-ladder-seg( is-lit)?"/g).map(Boolean);
    expect(lit.slice(0, 17).every(Boolean)).toBe(true);
    expect(lit.slice(17)).toEqual([false, false, true, false, false, false, false]);
  });

  it('draws nothing pale without a trail', () => {
    expect(draw({ style: 'rows' })).not.toContain('vitals-ghost');
  });
});
