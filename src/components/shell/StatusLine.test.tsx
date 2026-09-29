import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { VitalsOptions } from '../../lib/session';
import type { Vitals } from '../../lib/stores/vitalsStore';
import type { CombatHealth } from '../../lib/vitalsView';
import frameCss from '../../styles/frame.css?raw';
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
};
const FIGHT: Vitals = {
  hp: 186,
  maxhp: 1020,
  mana: 344,
  maxmana: 800,
  move: 870,
  maxmove: 930,
  low: { hp: true, mana: false, move: false },
};
const GUARD: CombatHealth = { name: 'Blackwatch Guard', hp_pct: 38 };
const DEFAULTS: VitalsOptions = { values: 'current-max', meter: 'line', warn_thirds: false };

function draw(props: Partial<StatusVitalsProps> = {}, options: Partial<VitalsOptions> = {}) {
  return renderToStaticMarkup(
    <StatusVitals
      showVitals
      vitals={FIGHT}
      target="Blackwatch Guard"
      combat={GUARD}
      {...props}
      options={{ ...DEFAULTS, ...options }}
    />,
  );
}

/** Each value on the line with its tone, in order. */
function values(html: string): string[] {
  return [
    ...html.matchAll(/<span class="shell-status-value( is-(?:low|warn))?">([^<]*)<\/span>/g),
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

  it('sets the warn tone in frame.css', () => {
    const at = frameCss.indexOf('.shell-statusline .is-warn {');
    expect(at).toBeGreaterThanOrEqual(0);
    expect(frameCss.slice(at, frameCss.indexOf('}', at))).toContain('color: var(--warn)');
  });
});
