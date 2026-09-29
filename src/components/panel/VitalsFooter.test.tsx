import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { VitalsDensity, VitalsOptions } from '../../lib/session';
import type { CombatOpponent } from '../../lib/stores/combatStore';
import type { Vitals } from '../../lib/stores/vitalsStore';
import panelCss from '../../styles/panel.css?raw';
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

const DEFAULTS: VitalsOptions = { values: 'current-max', meter: 'line', warn_thirds: false };

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
      options={{ ...DEFAULTS, ...options }}
    />,
  );
}

/** Each drawn value with its row's tone class, in order. */
function values(html: string): { value: string; tone: string }[] {
  const out: { value: string; tone: string }[] = [];
  const re =
    /<div class="(panel-vitals-(?:row|item)[^"]*)">.*?<span class="panel-vitals-value">([^<]*)<\/span>/g;
  for (const m of html.matchAll(re)) {
    const tone = /panel-vitals-row-(low|warn|combat)/.exec(m[1]);
    out.push({ value: m[2], tone: tone ? tone[1] : 'quiet' });
  }
  return out;
}

const meters = (html: string) => html.match(/class="panel-vitals-meter"/g)?.length ?? 0;

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
}

describe('VitalsBlock', () => {
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
