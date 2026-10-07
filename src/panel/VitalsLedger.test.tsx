import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import { PaneTextSizeContext } from './paneTextSize';
import { VitalsBlock } from './VitalsFooter';
import type { LedgerFit } from './vitalsLedgerFit';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// Board 1's fight: Tolliver at 765 of 1020, Mana and Moves full, and a
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
  options: Partial<VitalsOptions> = {},
  {
    fit = 'full',
    vitals = FIGHT,
    combat = GUARD,
    size = 12,
    opponentOnly = false,
  }: {
    fit?: LedgerFit;
    vitals?: Vitals | null;
    combat?: CombatOpponent | null;
    size?: number;
    opponentOnly?: boolean;
  } = {},
): string {
  return renderToStaticMarkup(
    <PaneTextSizeContext.Provider value={size}>
      <VitalsBlock
        vitals={vitals}
        combat={combat}
        fit={{ style: 'ledger', fit }}
        options={{ ...DEFAULT_VITALS_OPTIONS, ...options }}
        opponentOnly={opponentOnly}
      />
    </PaneTextSizeContext.Provider>,
  );
}

/** Each column's label, figure, max and tone, in order. */
function columns(html: string) {
  const re =
    /<div class="vitals-ledger-column vitals-tone( is-\w+)?"[^>]*><span class="vitals-ledger-label">([^<]*)<\/span><span class="vitals-ledger-figure"><span class="vitals-ledger-current">([^<]*)<\/span>(?:<span class="vitals-ledger-max">([^<]*)<\/span>)?/g;
  return [...html.matchAll(re)].map((m) => ({
    label: m[2],
    figure: m[3],
    max: m[4] ?? null,
    tone: m[1]?.trim().slice(3) ?? 'quiet',
  }));
}

/** The opponent's name and health, and where it sits. */
function opponent(html: string) {
  const m =
    /<div class="vitals-ledger-opponent vitals-tone is-foe([^"]*)"><span class="vitals-ledger-name">([^<]*)<\/span><span class="vitals-ledger-figure"><span class="vitals-ledger-current">([^<]*)</.exec(
      html,
    );
  return m && { name: m[2], health: m[3], classes: m[1].trim() };
}

const lines = (html: string) => html.match(/class="vitals-ledger-line"/g)?.length ?? 0;

describe('Ledger', () => {
  it('draws board 1 in a fight, the guard on top and a column for each vital', () => {
    const html = draw();
    expect(html).toContain('class="panel-vitals panel-vitals-ledger"');
    expect(opponent(html)).toEqual({ name: 'a Blackwatch guard', health: '54%', classes: '' });
    expect(html.indexOf('vitals-ledger-opponent')).toBeLessThan(
      html.indexOf('vitals-ledger-columns'),
    );
    expect(columns(html)).toEqual([
      { label: 'Health', figure: '765', max: '/ 1020', tone: 'quiet' },
      { label: 'Mana', figure: '800', max: '/ 800', tone: 'quiet' },
      { label: 'Moves', figure: '930', max: '/ 930', tone: 'quiet' },
    ]);
    // A line under each column and under the guard, on the Meter's px.
    expect(lines(html)).toBe(4);
    expect(html).toContain('--vitals-meter:2px');
    expect(html).toContain('--vitals-figure:16px');
    // It holds no height of its own once your vitals show.
    expect(html).not.toContain('--vitals-min-height');
  });

  it('turns a low vital danger and the middle third warn under the warning', () => {
    expect(columns(draw({}, { vitals: LOW, combat: null })).map((c) => c.tone)).toEqual([
      'low',
      'quiet',
      'quiet',
    ]);
    expect(
      columns(draw({ warn_thirds: true }, { vitals: LOW, combat: null })).map((c) => c.tone),
    ).toEqual(['low', 'warn', 'warn']);
  });

  it('drops the max and steps the figure as the fit asks', () => {
    for (const fit of ['bare', 'smaller', 'text'] as const) {
      expect(columns(draw({}, { fit })).every((c) => c.max === null)).toBe(true);
    }
    expect(draw({}, { fit: 'bare', size: 16 })).toContain('--vitals-figure:21px');
    expect(draw({}, { fit: 'smaller', size: 16 })).toContain('--vitals-figure:19px');
    expect(draw({}, { fit: 'text', size: 16 })).toContain('--vitals-figure:16px');
  });

  it('writes each figure in your Values form', () => {
    expect(columns(draw({ values: 'percent' }, { combat: null })).map((c) => c.figure)).toEqual([
      '75%',
      '100%',
      '100%',
    ]);
    expect(columns(draw({ values: 'current' }, { combat: null })).map((c) => c.max)).toEqual([
      null,
      null,
      null,
    ]);
  });

  it('draws Moves first with Mana off, and the guard at the bottom', () => {
    const html = draw({ order: ['move', 'hp', 'mana'], off: ['mana'], opponent: 'bottom' });
    expect(columns(html).map((c) => c.label)).toEqual(['Moves', 'Health']);
    expect(opponent(html)?.classes).toBe('is-bottom');
    expect(html.indexOf('vitals-ledger-columns')).toBeLessThan(
      html.indexOf('vitals-ledger-opponent'),
    );
  });

  it('keeps only the guard while your pinned prompt hides your vitals', () => {
    const html = draw({}, { opponentOnly: true });
    expect(columns(html)).toEqual([]);
    expect(opponent(html)?.classes).toBe('is-alone');
    expect(html).toContain('aria-label="Opponent"');
    expect(draw({}, { opponentOnly: true, combat: null })).toBe('');
  });

  it('reads ? over empty lines under lamented tears', () => {
    const html = draw({}, { vitals: HIDDEN, combat: { ...GUARD, hp_pct: null, hidden: true } });
    expect(columns(html)).toEqual([
      { label: 'Health', figure: '?', max: '/ ?', tone: 'hidden' },
      { label: 'Mana', figure: '?', max: '/ ?', tone: 'hidden' },
      { label: 'Moves', figure: '?', max: '/ ?', tone: 'hidden' },
    ]);
    expect(opponent(html)).toEqual({
      name: 'a Blackwatch guard',
      health: '?',
      classes: 'is-hidden',
    });
    expect(html).not.toContain('vitals-ledger-fill');
  });

  it('reads ? for a health the game withholds, never a blank figure', () => {
    const html = draw({}, { combat: { ...GUARD, hp_pct: null } });
    expect(opponent(html)?.health).toBe('?');
  });

  it('draws no lines under None', () => {
    expect(lines(draw({ meter: 'none' }))).toBe(0);
    expect(draw({ meter: 'bar' })).toContain('--vitals-meter:4px');
  });

  it('colors the label and line of a vital you gave a color', () => {
    const html = renderToStaticMarkup(
      <VitalsBlock
        vitals={FIGHT}
        combat={null}
        fit={{ style: 'ledger', fit: 'full' }}
        options={DEFAULT_VITALS_OPTIONS}
        inks={{ mana: '#8cc2d8' }}
      />,
    );
    expect(html.match(/--vital-ink/g)).toHaveLength(1);
    expect(html).toMatch(/style="--vital-ink:#8cc2d8"><span class="vitals-ledger-label">Mana/);
  });

  it('holds the room of its columns while it waits for your vitals', () => {
    const html = draw({}, { vitals: null, combat: null });
    expect(html).toContain('Vitals appear when you log in.');
    expect(html).toContain('--vitals-min-height:64px');
  });
});
