import { describe, expect, it } from 'vitest';
import { moonLabel, moonPhaseWord, parseMoons, parseWorldTime } from './worldStore';

describe('parseWorldTime', () => {
  it('reads the Aabahran payload, which has no minute', () => {
    expect(
      parseWorldTime({
        hour: 8,
        day: 12,
        month: 3,
        year: 1042,
        sunlight: 'light',
        sky: 'cloudy',
      }),
    ).toEqual({
      hour: 8,
      minute: null,
      day: 12,
      month: 3,
      year: 1042,
      sunlight: 'light',
      sky: 'cloudy',
    });
  });

  it('reads string numbers and a minute from servers that send one', () => {
    expect(parseWorldTime({ hour: '20', minute: 42 })).toMatchObject({ hour: 20, minute: 42 });
    expect(parseWorldTime({ hour: 5, min: '7' })?.minute).toBe(7);
  });

  it('drops out of range values', () => {
    expect(parseWorldTime({ hour: 24, minute: 60 })).toMatchObject({ hour: null, minute: null });
    expect(parseWorldTime('noon')).toBeNull();
  });
});

describe('parseMoons', () => {
  it('reads the three Aabahran moons and the sky flags', () => {
    const moons = parseMoons({
      moons: [
        { name: 'Lysenties', active: true, phase: 2, phase_name: 'half-lit and growing' },
        { name: 'Nercuros', active: false, phase: 4, phase_name: 'full and whole' },
        { name: '', active: true, phase: 1 },
      ],
      eclipse: false,
      triad: false,
      near_alignment: true,
    });
    expect(moons).toEqual({
      moons: [
        { name: 'Lysenties', active: true, phase: 2, phase_name: 'half-lit and growing' },
        { name: 'Nercuros', active: false, phase: 4, phase_name: 'full and whole' },
      ],
      eclipse: false,
      triad: false,
      near_alignment: true,
    });
  });
});

describe('moon labels', () => {
  it('names each phase with one word', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 7].map(moonPhaseWord)).toEqual([
      'new',
      'waxing',
      'waxing',
      'waxing',
      'full',
      'waning',
      'waning',
      'waning',
    ]);
    expect(moonPhaseWord(null)).toBeNull();
  });

  it('labels the first moon in the sky, else the first listed', () => {
    const base = { eclipse: false, triad: false, near_alignment: false };
    const moons = [
      { name: 'Lysenties', active: false, phase: 6, phase_name: null },
      { name: 'Nercuros', active: true, phase: 1, phase_name: null },
    ];
    expect(moonLabel({ ...base, moons })).toBe('Nercuros waxing');
    expect(moonLabel({ ...base, moons: [{ ...moons[0] }] })).toBe('Lysenties waning');
    expect(moonLabel({ ...base, moons: [] })).toBeNull();
    expect(moonLabel(null)).toBeNull();
  });
});
