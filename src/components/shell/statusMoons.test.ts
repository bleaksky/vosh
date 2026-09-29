import { describe, expect, it } from 'vitest';
import type { Moons } from '../../lib/stores/worldStore';
import { findTheme, themeTokens } from '../../lib/themes';
import { moonColor } from './moonColors';
import { statusMoons } from './statusMoons';

const nord = findTheme('nord');
const tokens = themeTokens(nord);

function sky(active: [boolean, boolean, boolean], flags: Partial<Moons> = {}): Moons {
  return {
    moons: [
      { name: 'Lysenties', active: active[0], phase: 2, phase_name: 'half-lit and growing' },
      { name: 'Nercuros', active: active[1], phase: 4, phase_name: 'full and whole' },
      { name: 'Dyphrities', active: active[2], phase: 7, phase_name: null },
    ],
    eclipse: false,
    triad: false,
    near_alignment: false,
    ...flags,
  };
}

describe('statusMoons', () => {
  it('draws the moons as ink on a light theme only', () => {
    expect(statusMoons(sky([true, true, true]), nord.xterm, tokens)?.onLight).toBe(false);
    const vellum = findTheme('vellum');
    expect(statusMoons(sky([true, true, true]), vellum.xterm, themeTokens(vellum))?.onLight).toBe(
      true,
    );
  });

  it('keeps only the moons in the sky, in the server order', () => {
    const out = statusMoons(sky([true, false, true]), nord.xterm, tokens);
    expect(out?.moons.map((moon) => moon.name)).toEqual(['Lysenties', 'Dyphrities']);
  });

  it('colors each moon from the theme and words it from the phase', () => {
    const out = statusMoons(sky([true, true, true]), nord.xterm, tokens);
    expect(out?.moons).toEqual([
      {
        name: 'Lysenties',
        phase: 2,
        color: moonColor('Lysenties', nord.xterm, tokens),
        label: 'Lysenties, half-lit and growing',
      },
      {
        name: 'Nercuros',
        phase: 4,
        color: moonColor('Nercuros', nord.xterm, tokens),
        label: 'Nercuros, full and whole',
      },
      {
        name: 'Dyphrities',
        phase: 7,
        color: moonColor('Dyphrities', nord.xterm, tokens),
        label: 'Dyphrities, waning',
      },
    ]);
    expect(out?.alignment).toBeNull();
  });

  it('names one sky event', () => {
    const all: [boolean, boolean, boolean] = [true, true, true];
    const out = statusMoons(sky(all, { triad: true, near_alignment: true }), nord.xterm, tokens);
    expect(out?.alignment).toBe('Triad');
  });

  it('shows nothing when no moon is in the sky', () => {
    expect(statusMoons(null, nord.xterm, tokens)).toBeNull();
    expect(statusMoons(sky([false, false, false], { triad: true }), nord.xterm, tokens)).toBeNull();
  });
});
