import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfigVitals';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import { PaneTextSizeContext } from './paneTextSize';
import { VitalsBlock } from './VitalsFooter';
import type { VitalsFit } from './vitalsFit';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// A fight: Tolliver worn at 765 of 1020 and low at 159, and a
// Blackwatch guard at 54 percent.
const FIGHT: Vitals = {
  hp: 765,
  maxhp: 1020,
  mana: 800,
  maxmana: 800,
  move: 930,
  maxmove: 930,
  low: { hp: false, mana: false, move: false },
  hidden: false,
};
const LOW: Vitals = {
  ...FIGHT,
  hp: 159,
  mana: 310,
  move: 489,
  low: { hp: true, mana: false, move: false },
};
const HIDDEN: Vitals = {
  hp: 0,
  maxhp: 0,
  mana: 0,
  maxmana: 0,
  move: 0,
  maxmove: 0,
  low: { hp: false, mana: false, move: false },
  hidden: true,
};
const GUARD: CombatOpponent = {
  name: 'a Blackwatch guard',
  hp_pct: 54,
  condition: null,
  hidden: false,
  tank: null,
};

function draw(
  fit: VitalsFit,
  options: Partial<VitalsOptions> = {},
  {
    vitals = FIGHT,
    combat = GUARD,
    opponentOnly = false,
  }: { vitals?: Vitals | null; combat?: CombatOpponent | null; opponentOnly?: boolean } = {},
): string {
  return renderToStaticMarkup(
    <PaneTextSizeContext.Provider value={12}>
      <VitalsBlock
        vitals={vitals}
        combat={combat}
        fit={fit}
        options={{ ...DEFAULT_VITALS_OPTIONS, ...options }}
        opponentOnly={opponentOnly}
      />
    </PaneTextSizeContext.Provider>,
  );
}

const GAUGES: VitalsFit = { style: 'gauges', fit: 'beside' };
const PIPS: VitalsFit = { style: 'pips', fit: 'ten' };

/** Each row's label, value, widest value and tone, in order. */
function rows(html: string) {
  const re =
    /<div class="vitals-mark-row (?:vitals-mark-opponent )?vitals-tone( is-[\w -]+)?"[^>]*><span class="vitals-mark-label">([^<]*)<\/span>.*?<span class="vitals-mark-widest" aria-hidden="true">([^<]*)<\/span><span>([^<]*)<\/span>/g;
  return [...html.matchAll(re)].map((m) => ({
    label: m[2],
    value: m[4],
    widest: m[3],
    tone: m[1]?.trim() ?? '',
  }));
}

/** Each pill's fill in percent, null for an empty pill. */
function pills(html: string) {
  return [...html.matchAll(/<span class="vitals-gauge" aria-hidden="true">(.*?)<\/span>/g)].map(
    (m) => /width:([\d.]+)%/.exec(m[1])?.[1] ?? null,
  );
}

/** Each vital's discs, F full, H half, . dark. */
function discs(html: string) {
  return [
    ...html.matchAll(
      /<span class="vitals-pips" aria-hidden="true">(.*?)<\/span><span class="vitals-mark-value"/g,
    ),
  ].map((m) =>
    [...m[1].matchAll(/class="vitals-pip( is-(full|half))?"/g)]
      .map((d) => (d[2] === 'full' ? 'F' : d[2] === 'half' ? 'H' : '.'))
      .join(''),
  );
}

describe('Gauges', () => {
  it('draws board 1 in a fight, the guard on top with no pill', () => {
    const html = draw(GAUGES);
    expect(html).toContain('class="panel-vitals panel-vitals-marks"');
    expect(html).toContain('class="vitals-marks is-gauges"');
    expect(rows(html)).toEqual([
      { label: 'a Blackwatch guard', value: '54%', widest: '100%', tone: 'is-foe' },
      { label: 'Health', value: '765 / 1020', widest: '1020 / 1020', tone: '' },
      { label: 'Mana', value: '800 / 800', widest: '800 / 800', tone: '' },
      { label: 'Moves', value: '930 / 930', widest: '930 / 930', tone: '' },
    ]);
    expect(pills(html)).toEqual(['75', '100', '100']);
    // Gauges draws its own mark, so Meter leaves it alone.
    expect(draw(GAUGES, { meter: 'none' })).toBe(html);
    expect(html).not.toContain('--vitals-min-height');
  });

  it('turns a low vital danger, the warning the middle third warn', () => {
    expect(rows(draw(GAUGES, {}, { vitals: LOW, combat: null })).map((r) => r.tone)).toEqual([
      'is-low',
      '',
      '',
    ]);
    expect(
      rows(draw(GAUGES, { warn_thirds: true }, { vitals: LOW, combat: null })).map((r) => r.tone),
    ).toEqual(['is-low', 'is-warn', 'is-warn']);
  });

  it('drops each pill under its label and value when the fit asks', () => {
    const html = draw({ style: 'gauges', fit: 'under' });
    expect(html).toContain('class="panel-vitals panel-vitals-marks is-under"');
    expect(html).toContain('class="vitals-marks is-gauges is-under"');
    expect(pills(html)).toHaveLength(3);
  });

  it('draws Moves first, and the guard at the bottom', () => {
    const html = draw(GAUGES, { order: ['move', 'hp', 'mana'], opponent: 'bottom' });
    expect(rows(html).map((r) => r.label)).toEqual([
      'Moves',
      'Health',
      'Mana',
      'a Blackwatch guard',
    ]);
  });

  it('keeps only the guard while your pinned prompt hides your vitals', () => {
    const html = draw(GAUGES, {}, { opponentOnly: true });
    expect(rows(html).map((r) => r.label)).toEqual(['a Blackwatch guard']);
    expect(html).toContain('aria-label="Opponent"');
    expect(draw(GAUGES, {}, { opponentOnly: true, combat: null })).toBe('');
  });

  it('reads ? over empty pills under lamented tears', () => {
    const html = draw(
      GAUGES,
      {},
      { vitals: HIDDEN, combat: { ...GUARD, hp_pct: null, hidden: true } },
    );
    expect(rows(html).map((r) => [r.value, r.tone])).toEqual([
      ['?', 'is-foe is-hidden'],
      ['? / ?', 'is-hidden'],
      ['? / ?', 'is-hidden'],
      ['? / ?', 'is-hidden'],
    ]);
    expect(pills(html)).toEqual([null, null, null]);
  });

  it('reads ? for a health the game withholds', () => {
    expect(rows(draw(GAUGES, {}, { combat: { ...GUARD, hp_pct: null } }))[0].value).toBe('?');
  });

  it('colors the label and pill of a vital you gave a color', () => {
    const html = renderToStaticMarkup(
      <VitalsBlock
        vitals={LOW}
        combat={null}
        fit={GAUGES}
        options={DEFAULT_VITALS_OPTIONS}
        inks={{ mana: '#d2bbff' }}
      />,
    );
    expect(html.match(/--vital-ink/g)).toHaveLength(1);
    expect(html).toMatch(/style="--vital-ink:#d2bbff"><span class="vitals-mark-label">Mana/);
  });

  it('holds a row for each vital while it waits for your vitals', () => {
    const html = draw(GAUGES, {}, { vitals: null, combat: null });
    expect(html).toContain('Vitals appear when you log in.');
    expect(html).toContain('--vitals-min-height:87px');
  });
});

describe('Pips', () => {
  it('lights ten discs in halves beside each value', () => {
    const html = draw(PIPS, {}, { vitals: LOW, combat: null });
    expect(html).toContain('class="vitals-marks is-pips"');
    expect(discs(html)).toEqual(['FH........', 'FFFF......', 'FFFFFH....']);
  });

  it('lights five discs at a narrow width, and ten under the label at the worst', () => {
    expect(discs(draw({ style: 'pips', fit: 'five' }))).toEqual(['FFFF.', 'FFFFF', 'FFFFF']);
    const under = draw({ style: 'pips', fit: 'under' });
    expect(under).toContain('class="vitals-marks is-pips is-under"');
    expect(discs(under)).toEqual(['FFFFFFFH..', 'FFFFFFFFFF', 'FFFFFFFFFF']);
  });

  it('draws the guard as a name and health with no discs', () => {
    const html = draw(PIPS);
    expect(rows(html)[0]).toEqual({
      label: 'a Blackwatch guard',
      value: '54%',
      widest: '100%',
      tone: 'is-foe',
    });
    expect(discs(html)).toHaveLength(3);
  });

  it('lights no disc under lamented tears', () => {
    const html = draw(PIPS, {}, { vitals: HIDDEN, combat: null });
    expect(discs(html)).toEqual(['..........', '..........', '..........']);
  });
});
