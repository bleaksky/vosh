import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfig';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import type { CombatHealth } from '../panel/vitalsView';
import frameCss from '../styles/frame.css?raw';
import { StatusVitals, type StatusVitalsProps } from './StatusLine';

// The stores behind StatusLine reach the Tauri bridge. StatusVitals,
// under test, draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The VitalsOptions board's status line cases.
const FULL: Vitals = {
  hp: 1020,
  maxhp: 1020,
  mana: 800,
  maxmana: 800,
  move: 930,
  maxmove: 930,
  low: { hp: false, mana: false, move: false },
  hidden: false,
};
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
// Lamented tears: Char.Vitals all zeros with the hidden flag.
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
const GUARD: CombatHealth = { name: 'Blackwatch Guard', hp_pct: 38 };

function draw(props: Partial<StatusVitalsProps> = {}, options: Partial<VitalsOptions> = {}) {
  return renderToStaticMarkup(
    <StatusVitals
      showVitals
      vitals={FIGHT}
      target="Blackwatch Guard"
      combat={GUARD}
      {...props}
      options={{ ...DEFAULT_VITALS_OPTIONS, ...options }}
    />,
  );
}

/** Each value on the line with its tone, in order. */
function values(html: string): string[] {
  return [
    ...html.matchAll(/<span class="shell-status-value( is-(?:low|warn|hidden))?">([^<]*)<\/span>/g),
  ].map((m) => (m[1] ? `${m[2]} ${m[1].trim()}` : m[2]));
}

describe('StatusVitals', () => {
  it('reads current and max while you explore', () => {
    expect(values(draw({ vitals: FULL, combat: null }))).toEqual([
      '1020 / 1020',
      '800 / 800',
      '930 / 930',
      'Blackwatch Guard',
    ]);
  });

  it('turns low Health danger in a fight and shows the target health in warn', () => {
    expect(values(draw())).toEqual([
      '186 / 1020 is-low',
      '344 / 800',
      '870 / 930',
      'Blackwatch Guard',
      '38% is-warn',
    ]);
  });

  it('follows Values', () => {
    expect(values(draw({}, { values: 'current' })).slice(0, 3)).toEqual([
      '186 is-low',
      '344',
      '870',
    ]);
    expect(values(draw({}, { values: 'percent' })).slice(0, 3)).toEqual([
      '18% is-low',
      '43%',
      '94%',
    ]);
  });

  it('follows Warn before you run low', () => {
    expect(values(draw({}, { warn_thirds: true })).slice(0, 3)).toEqual([
      '186 / 1020 is-low',
      '344 / 800 is-warn',
      '870 / 930',
    ]);
  });

  it('never draws a meter', () => {
    for (const meter of ['line', 'bar', 'none'] as const) {
      expect(draw({}, { meter })).toBe(draw());
    }
    expect(draw()).not.toContain('meter');
  });

  it('shows the target health only when you fight that target', () => {
    expect(values(draw({ target: 'blackwatch guard' }))).toContain('38% is-warn');
    expect(values(draw({ target: 'guard' }))).not.toContain('38% is-warn');
    expect(values(draw({ combat: null }))).not.toContain('38% is-warn');
    expect(values(draw({ combat: { ...GUARD, hp_pct: null } }))).not.toContain('38% is-warn');
  });

  it('keeps the target by name alone while the panel shows', () => {
    expect(values(draw({ showVitals: false }))).toEqual(['Blackwatch Guard']);
  });

  it('draws nothing with no vitals shown and no target', () => {
    expect(draw({ showVitals: false, target: null })).toBe('');
  });

  it('shows ? for each hidden vital in the quiet tone and never warns', () => {
    for (const warn_thirds of [false, true]) {
      expect(values(draw({ vitals: HIDDEN, combat: null }, { warn_thirds }))).toEqual([
        '? / ? is-hidden',
        '? / ? is-hidden',
        '? / ? is-hidden',
        'Blackwatch Guard',
      ]);
    }
    expect(values(draw({ vitals: HIDDEN }, { values: 'current' })).slice(0, 3)).toEqual([
      '? is-hidden',
      '? is-hidden',
      '? is-hidden',
    ]);
    expect(values(draw({ vitals: HIDDEN }, { values: 'percent' })).slice(0, 3)).toEqual([
      '?% is-hidden',
      '?% is-hidden',
      '?% is-hidden',
    ]);
    const html = draw({ vitals: HIDDEN, combat: null });
    for (const label of ['Health', 'Mana', 'Moves']) expect(html).toContain(label);
  });

  it('drops the target health while the game withholds it', () => {
    expect(values(draw({ combat: { ...GUARD, hidden: true } }))).toEqual([
      '186 / 1020 is-low',
      '344 / 800',
      '870 / 930',
      'Blackwatch Guard',
    ]);
    expect(values(draw({ combat: { ...GUARD, hp_pct: null } }))).not.toContain('38% is-warn');
  });

  it('sets the hidden tone in frame.css', () => {
    const at = frameCss.indexOf('.shell-status-value.is-hidden {');
    expect(at).toBeGreaterThanOrEqual(0);
    expect(frameCss.slice(at, frameCss.indexOf('}', at))).toContain('color: var(--tertiary)');
  });

  it('lets your target give way with an ellipsis and keeps every other item whole', () => {
    expect(draw()).toContain('<span class="shell-status-target">Target<span');
    const rule = (selector: string) => {
      const at = frameCss.indexOf(`\n${selector} {`);
      expect(at, selector).toBeGreaterThanOrEqual(0);
      return frameCss.slice(at, frameCss.indexOf('}', at));
    };
    expect(rule('.shell-statusline > *')).toContain('flex: none;');
    const target = rule('.shell-statusline > .shell-status-target');
    expect(target).toContain('flex: 0 1 auto;');
    expect(target).toContain('min-width: 0;');
    expect(target).toContain('overflow: hidden;');
    expect(target).toContain('text-overflow: ellipsis;');
  });

  it('sets the warn tone in frame.css', () => {
    const at = frameCss.indexOf('.shell-statusline .is-warn {');
    expect(at).toBeGreaterThanOrEqual(0);
    expect(frameCss.slice(at, frameCss.indexOf('}', at))).toContain('color: var(--warn)');
  });
});
