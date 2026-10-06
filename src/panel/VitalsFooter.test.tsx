import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsDensity, type VitalsOptions } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import panelCss from '../styles/panel.css?raw';
import { PaneTextSizeContext } from './paneTextSize';
import type { VitalsLineFit } from './vitalsLine';
import { VitalsBlock } from './VitalsFooter';

// The stores behind VitalsFooter reach the Tauri bridge. VitalsBlock,
// under test, draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The VitalsOptions board's fight, in Nord: Blackwatch Guard at 38%,
// Health 186 / 1020 (low), Mana 344 / 800, Moves 870 / 930.
const FIGHT: Vitals = {
  hp: 186,
  maxhp: 1020,
  mana: 344,
  maxmana: 800,
  move: 870,
  maxmove: 930,
  low: { hp: true, mana: false, move: false },
  hidden: false,
};
const GUARD: CombatOpponent = {
  name: 'Blackwatch Guard',
  hp_pct: 38,
  condition: null,
  hidden: false,
  tank: null,
};

function draw(
  options: Partial<VitalsOptions> = {},
  {
    density = 'rows',
    fit = 'rows',
    vitals = FIGHT,
    combat = GUARD,
  }: {
    density?: VitalsDensity;
    fit?: VitalsLineFit;
    vitals?: Vitals | null;
    combat?: CombatOpponent | null;
  } = {},
): string {
  return renderToStaticMarkup(
    <VitalsBlock
      vitals={vitals}
      combat={combat}
      density={density}
      fit={fit}
      options={{ ...DEFAULT_VITALS_OPTIONS, ...options }}
    />,
  );
}

// Lamented tears: Char.Vitals all zeros with the hidden flag, and a
// Char.Combat that names the guard and withholds its health.
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
const GUARD_HIDDEN: CombatOpponent = {
  name: 'Blackwatch Guard',
  hp_pct: null,
  condition: null,
  hidden: true,
  tank: null,
};

/** Each drawn value with the tone class of the row or One line item
 *  that holds it, in order. */
function values(html: string): { value: string; tone: string }[] {
  const out: { value: string; tone: string }[] = [];
  const re =
    /<div class="(panel-vitals-(?:row|item)[^"]*)"><div class="panel-vitals-line">.*?<span class="panel-vitals-value">([^<]*)<\/span>/g;
  for (const m of html.matchAll(re)) {
    const tone =
      /panel-vitals-row-(hidden)/.exec(m[1]) ?? /panel-vitals-row-(low|warn|combat)/.exec(m[1]);
    out.push({ value: m[2], tone: tone ? tone[1] : 'quiet' });
  }
  return out;
}

const meters = (html: string) => html.match(/class="panel-vitals-meter"/g)?.length ?? 0;
const fills = (html: string) => html.match(/class="panel-vitals-fill"/g)?.length ?? 0;

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
}

describe('VitalsBlock', () => {
  it('keeps only the opponent row while your pinned prompt hides your vitals', () => {
    const only = (combat: CombatOpponent | null) =>
      renderToStaticMarkup(
        <VitalsBlock
          vitals={FIGHT}
          combat={combat}
          density="rows"
          fit="rows"
          options={DEFAULT_VITALS_OPTIONS}
          opponentOnly
        />,
      );
    expect(values(only(GUARD))).toEqual([{ value: '38%', tone: 'combat' }]);
    expect(only(GUARD)).toContain('--vitals-min-height');
    // Out of a fight it draws nothing, so the panes keep the room.
    expect(only(null)).toBe('');
  });

  it('draws the panel you have today with the defaults', () => {
    const html = draw();
    expect(values(html)).toEqual([
      { value: '38%', tone: 'combat' },
      { value: '186 / 1020', tone: 'low' },
      { value: '344 / 800', tone: 'quiet' },
      { value: '870 / 930', tone: 'quiet' },
    ]);
    expect(meters(html)).toBe(4);
    expect(html).toContain('--vitals-row:28px');
    expect(html).toContain('--vitals-meter:2px');
    expect(html).toContain('--vitals-meter-gap:1px');
    expect(html).toContain('--vitals-min-height:104px');
  });

  it('writes the values as Values asks', () => {
    expect(values(draw({ values: 'current' })).map((v) => v.value)).toEqual([
      '38%',
      '186',
      '344',
      '870',
    ]);
    expect(values(draw({ values: 'percent' })).map((v) => v.value)).toEqual([
      '38%',
      '18%',
      '43%',
      '94%',
    ]);
  });

  it('turns Mana warn in the middle third when Warn before you run low is on', () => {
    expect(values(draw({ warn_thirds: true })).map((v) => v.tone)).toEqual([
      'combat',
      'low',
      'warn',
      'quiet',
    ]);
  });

  it('draws the 4 px bar 2 under the text on the same pitch', () => {
    const html = draw({ meter: 'bar' });
    expect(meters(html)).toBe(4);
    expect(html).toContain('--vitals-row:28px');
    expect(html).toContain('--vitals-meter:4px');
    expect(html).toContain('--vitals-meter-gap:2px');
    expect(html).toContain('--vitals-meter-radius:2px');
  });

  it('drops every meter and tightens the rows for None', () => {
    const html = draw({ values: 'current', meter: 'none', warn_thirds: true });
    expect(meters(html)).toBe(0);
    expect(html).toContain('--vitals-row:22px');
    expect(html).toContain('--vitals-row-top:3px');
    expect(html).toContain('--vitals-pad-top:9px');
    expect(html).toContain('--vitals-pad-bottom:13px');
    expect(values(html)).toEqual([
      { value: '38%', tone: 'combat' },
      { value: '186', tone: 'low' },
      { value: '344', tone: 'warn' },
      { value: '870', tone: 'quiet' },
    ]);
  });

  it('sets One line with its labels, each value over its own bar', () => {
    const html = draw({ values: 'percent', meter: 'bar' }, { density: 'line', fit: 'labels' });
    expect(html).toContain('panel-vitals is-one-line');
    expect(html).toContain('panel-vitals-oneline');
    expect(html).not.toContain('is-bare');
    expect(html.match(/class="panel-vitals-label"/g)).toHaveLength(4);
    expect(meters(html)).toBe(4);
    expect(html).toContain('--vitals-min-height:48px');
  });

  it('keeps the names for a screen reader when One line drops the labels', () => {
    const html = draw({}, { density: 'line', fit: 'values' });
    expect(html.match(/panel-vitals-item is-bare/g)).toHaveLength(3);
    expect(html).toContain('<span class="panel-vitals-label-hidden">Health</span>');
  });

  it('draws One line without meters at the dense pitch', () => {
    const html = draw({ meter: 'none' }, { density: 'line', fit: 'labels', combat: null });
    expect(meters(html)).toBe(0);
    expect(html).toContain('--vitals-min-height:45px');
  });

  it('stacks One line in rows when even the values do not fit', () => {
    const html = draw({}, { density: 'line', fit: 'rows' });
    expect(html).not.toContain('panel-vitals-oneline');
    expect(values(html)).toHaveLength(4);
  });

  it('holds the height it will need while it waits for your vitals', () => {
    expect(draw({}, { vitals: null, combat: null })).toContain('--vitals-min-height:104px');
    expect(draw({ meter: 'none' }, { vitals: null, combat: null })).toContain(
      '--vitals-min-height:89px',
    );
    expect(draw({}, { density: 'line', vitals: null, combat: null })).toContain(
      '--vitals-min-height:48px',
    );
    expect(draw({}, { vitals: null, combat: null })).toContain('Vitals appear when you log in.');
  });

  it('draws ? for each hidden vital, quiet, with empty meters, in every option', () => {
    const forms = { 'current-max': '? / ?', current: '?', percent: '?%' } as const;
    for (const density of ['rows', 'line'] as const) {
      for (const fit of ['labels', 'values', 'rows'] as const) {
        for (const form of ['current-max', 'current', 'percent'] as const) {
          for (const meter of ['line', 'bar', 'none'] as const) {
            for (const warn_thirds of [false, true]) {
              const html = draw(
                { values: form, meter, warn_thirds },
                { density, fit, vitals: HIDDEN, combat: null },
              );
              const at = `${density} ${fit} ${form} ${meter} ${warn_thirds}`;
              expect(values(html), at).toEqual([
                { value: forms[form], tone: 'hidden' },
                { value: forms[form], tone: 'hidden' },
                { value: forms[form], tone: 'hidden' },
              ]);
              expect(meters(html), at).toBe(meter === 'none' ? 0 : 3);
              expect(fills(html), at).toBe(0);
              expect(html, at).not.toMatch(/panel-vitals-row-(low|warn)/);
            }
          }
        }
      }
    }
  });

  it('keeps Health, Mana, and Moves while hidden, though the game sends every max as 0', () => {
    const html = draw({}, { vitals: HIDDEN, combat: null });
    for (const label of ['Health', 'Mana', 'Moves']) {
      expect(html).toContain(`<span class="panel-vitals-label">${label}</span>`);
    }
    expect(html).toContain('--vitals-min-height:104px');
  });

  it('shows your vitals again, low tone and all, once the game sends them', () => {
    expect(values(draw({}, { vitals: HIDDEN })).map((v) => v.value)).toEqual([
      '38%',
      '? / ?',
      '? / ?',
      '? / ?',
    ]);
    expect(values(draw()).map((v) => v.tone)).toEqual(['combat', 'low', 'quiet', 'quiet']);
  });

  it('draws the opponent health as ? in the quiet tone when the game withholds it', () => {
    const html = draw({}, { combat: GUARD_HIDDEN });
    expect(values(html)[0]).toEqual({ value: '?', tone: 'hidden' });
    expect(html).toContain('<span class="panel-vitals-label">Blackwatch Guard</span>');
    // The opponent's meter sits empty. Only your own three fill.
    expect(meters(html)).toBe(4);
    expect(fills(html)).toBe(3);
    // A condition that rode along never shows either.
    expect(values(draw({}, { combat: { ...GUARD_HIDDEN, condition: 'awful' } }))[0].value).toBe(
      '?',
    );
  });

  it('reads the guard as ? when Char.Combat sends neither a percent nor a condition', () => {
    // char-combat-withheld.gmcp, the target alone with no flag.
    const withheld: CombatOpponent = { ...GUARD, hp_pct: null };
    for (const density of ['rows', 'line'] as const) {
      const html = draw({}, { density, fit: 'labels', combat: withheld });
      expect(values(html)[0]).toEqual({ value: '?', tone: 'hidden' });
      expect(fills(html)).toBe(3);
    }
    // A condition with no percent still reads as the game words it.
    expect(values(draw({}, { combat: { ...withheld, condition: 'awful' } }))[0]).toEqual({
      value: 'awful',
      tone: 'combat',
    });
  });

  it('draws your vitals in your order without the ones you turned off', () => {
    const html = draw({ order: ['move', 'hp', 'mana'], off: ['mana'] }, { combat: null });
    expect(values(html).map((v) => v.value)).toEqual(['870 / 930', '186 / 1020']);
    // Two rows hold two rows' height, with no empty band under them.
    expect(html).toContain('--vitals-min-height:76px');
    const line = draw(
      { order: ['move', 'hp', 'mana'], off: ['mana'] },
      { density: 'line', fit: 'labels', combat: null },
    );
    expect(values(line).map((v) => v.value)).toEqual(['870 / 930', '186 / 1020']);
  });

  it('holds the height of the vitals the game sends a max for', () => {
    const html = draw({}, { vitals: { ...FIGHT, maxmana: 0, mana: 0 }, combat: null });
    expect(values(html).map((v) => v.value)).toEqual(['186 / 1020', '870 / 930']);
    expect(html).toContain('--vitals-min-height:76px');
    // While it waits it holds every vital you left on.
    expect(draw({ off: ['move'] }, { vitals: null, combat: null })).toContain(
      '--vitals-min-height:76px',
    );
  });

  it('puts your opponent at the bottom when you ask', () => {
    for (const density of ['rows', 'line'] as const) {
      const html = draw({ opponent: 'bottom' }, { density, fit: 'labels' });
      expect(values(html).map((v) => v.value)).toEqual([
        '186 / 1020',
        '344 / 800',
        '870 / 930',
        '38%',
      ]);
    }
  });

  it('drops your opponent when its switch is off', () => {
    expect(values(draw({ off: ['opponent'] })).map((v) => v.value)).toEqual([
      '186 / 1020',
      '344 / 800',
      '870 / 930',
    ]);
    const only = renderToStaticMarkup(
      <VitalsBlock
        vitals={FIGHT}
        combat={GUARD}
        density="rows"
        fit="rows"
        options={{ ...DEFAULT_VITALS_OPTIONS, off: ['opponent'] }}
        opponentOnly
      />,
    );
    expect(only).toBe('');
  });

  it('keeps only your opponent with all three off, and nothing out of a fight', () => {
    const off: VitalsOptions['off'] = ['hp', 'mana', 'move'];
    for (const density of ['rows', 'line'] as const) {
      const html = draw({ off }, { density, fit: 'labels' });
      expect(values(html)).toEqual([{ value: '38%', tone: 'combat' }]);
      expect(html).toContain('aria-label="Opponent"');
      expect(html).toContain('--vitals-min-height:48px');
      expect(draw({ off }, { density, combat: null })).toBe('');
      expect(draw({ off }, { density, vitals: null, combat: null })).toBe('');
    }
  });

  it('colors the label and the meter of a vital you gave a color, never its number', () => {
    for (const [density, fit] of [
      ['rows', 'rows'],
      ['line', 'labels'],
    ] as const) {
      const html = renderToStaticMarkup(
        <VitalsBlock
          vitals={FIGHT}
          combat={GUARD}
          density={density}
          fit={fit}
          options={DEFAULT_VITALS_OPTIONS}
          inks={{ mana: '#8cc2d8' }}
        />,
      );
      expect(html.match(/panel-vitals-swatch/g)).toHaveLength(1);
      expect(html).toMatch(/panel-vitals-swatch" style="--vital-ink:#8cc2d8"><div[^>]*>[^]*?Mana/);
    }
    expect(rule('.panel-vitals-swatch .panel-vitals-label')).toContain('color: var(--vital-ink)');
    expect(rule('.panel-vitals-swatch .panel-vitals-fill')).toContain(
      'background: var(--vital-ink)',
    );
    // Low and warn come after, so they still turn the meter.
    expect(panelCss.indexOf('.panel-vitals-swatch .panel-vitals-fill {')).toBeLessThan(
      panelCss.indexOf('.panel-vitals-row-low .panel-vitals-fill {'),
    );
    expect(panelCss).not.toMatch(/\.panel-vitals-swatch \.panel-vitals-value/);
  });

  it('sets the hidden tone in panel.css after the warn and combat tones', () => {
    const hidden = rule('.panel-vitals-row-hidden .panel-vitals-value');
    expect(hidden).toContain('color: var(--tertiary)');
    expect(panelCss.indexOf('.panel-vitals-row-hidden .panel-vitals-value {')).toBeGreaterThan(
      panelCss.indexOf('.panel-vitals-row-combat .panel-vitals-value {'),
    );
  });

  it('scales the rows and the space round them with your panel size, the meter as it is', () => {
    const at = (size: number, meter: VitalsOptions['meter']) =>
      renderToStaticMarkup(
        <PaneTextSizeContext.Provider value={size}>
          <VitalsBlock
            vitals={FIGHT}
            combat={null}
            density="rows"
            fit="rows"
            options={{ ...DEFAULT_VITALS_OPTIONS, meter }}
          />
        </PaneTextSizeContext.Provider>,
      );
    expect(at(12, 'line')).toBe(draw({}, { combat: null }));
    const line = at(16, 'line');
    expect(line).toContain('--vitals-row:37px');
    expect(line).toContain('--vitals-row-top:5px');
    expect(line).toContain('--vitals-meter:2px');
    expect(line).toContain('--vitals-meter-gap:1px');
    // 1 + 11 + 3 x 37 + 15.
    expect(line).toContain('--vitals-min-height:138px');
    const bar = at(16, 'bar');
    expect(bar).toContain('--vitals-meter:4px');
    expect(bar).toContain('--vitals-meter-gap:3px');
    const none = at(16, 'none');
    expect(none).toContain('--vitals-row:29px');
    expect(none).toContain('--vitals-row-top:4px');
  });

  it('reads the geometry and the warn tone in panel.css', () => {
    expect(rule('.panel-vitals-row')).toContain('height: var(--vitals-row, 28px)');
    expect(rule('.panel-vitals-meter')).toContain('height: var(--vitals-meter, 2px)');
    expect(rule('.panel-vitals-row-warn .panel-vitals-value')).toContain('color: var(--warn)');
    expect(rule('.panel-vitals-row-warn .panel-vitals-fill')).toContain('background: var(--warn)');
    expect(rule('.panel-vitals-item.is-bare .panel-vitals-line')).toContain(
      'justify-content: flex-end',
    );
  });
});
