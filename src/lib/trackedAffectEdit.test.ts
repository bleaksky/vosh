import { describe, expect, it } from 'vitest';
import type { TrackedAffect } from '../ipc/affects';
import {
  addTrackedAffect,
  affectSuggestions,
  isTracked,
  moveTrackedAffect,
  removeTrackedAffect,
  setTrackedAffectLabel,
  trackedAffectLabel,
} from './trackedAffectEdit';

const t = (name: string, label: string | null = null): TrackedAffect => ({ name, label });
const ILSABET = [t('Sanctuary'), t('haste'), t('giant strength'), t('Fly')];

describe('trackedAffectLabel', () => {
  it('shows your label, else the name exactly as the game sends it', () => {
    expect(trackedAffectLabel(t('giant strength'))).toBe('giant strength');
    expect(trackedAffectLabel(t('Protection evil'))).toBe('Protection evil');
    expect(trackedAffectLabel(t('the Triumph of One God'))).toBe('the Triumph of One God');
    expect(trackedAffectLabel(t('field of discord', 'Shroud'))).toBe('Shroud');
    expect(trackedAffectLabel(t('haste', '  '))).toBe('haste');
  });
});

describe('addTrackedAffect', () => {
  it('adds a new name last', () => {
    expect(addTrackedAffect(ILSABET, '  detect   invisible ')).toEqual([
      ...ILSABET,
      t('detect invisible'),
    ]);
  });

  it('keeps the list when the name is blank or already tracked in any case', () => {
    expect(addTrackedAffect(ILSABET, '   ')).toBe(ILSABET);
    expect(addTrackedAffect(ILSABET, 'HASTE')).toBe(ILSABET);
    expect(addTrackedAffect(ILSABET, 'Giant  Strength')).toBe(ILSABET);
    expect(isTracked(ILSABET, 'fly')).toBe(true);
    expect(isTracked(ILSABET, 'bless')).toBe(false);
  });
});

describe('removeTrackedAffect and moveTrackedAffect', () => {
  it('removes by index and ignores an index out of range', () => {
    expect(removeTrackedAffect(ILSABET, 1).map((e) => e.name)).toEqual([
      'Sanctuary',
      'giant strength',
      'Fly',
    ]);
    expect(removeTrackedAffect(ILSABET, 9)).toBe(ILSABET);
  });

  it('moves one place and stops at either end', () => {
    expect(moveTrackedAffect(ILSABET, 1, -1).map((e) => e.name)).toEqual([
      'haste',
      'Sanctuary',
      'giant strength',
      'Fly',
    ]);
    expect(moveTrackedAffect(ILSABET, 2, 1).map((e) => e.name)).toEqual([
      'Sanctuary',
      'haste',
      'Fly',
      'giant strength',
    ]);
    expect(moveTrackedAffect(ILSABET, 0, -1)).toBe(ILSABET);
    expect(moveTrackedAffect(ILSABET, 3, 1)).toBe(ILSABET);
  });
});

describe('setTrackedAffectLabel', () => {
  it('sets a trimmed label and clears a blank one', () => {
    const labeled = setTrackedAffectLabel(ILSABET, 2, '  Giant ');
    expect(labeled[2]).toEqual(t('giant strength', 'Giant'));
    expect(labeled[0]).toBe(ILSABET[0]);
    expect(setTrackedAffectLabel(labeled, 2, '')[2]).toEqual(t('giant strength'));
  });

  it('keeps the list when nothing changes', () => {
    expect(setTrackedAffectLabel(ILSABET, 0, ' ')).toBe(ILSABET);
    expect(setTrackedAffectLabel(ILSABET, 7, 'Nope')).toBe(ILSABET);
  });
});

describe('affectSuggestions', () => {
  const current = [
    { name: 'sanctuary' },
    { name: 'bless' },
    { name: 'armor' },
    { name: 'detect invisible' },
    { name: 'detect  magic' },
    { name: 'bless' },
    { name: 'protection evil' },
  ];

  it('offers what you have and do not track, alphabetically, named as the game sends them', () => {
    expect(affectSuggestions(current, ILSABET, '')).toEqual([
      { name: 'armor' },
      { name: 'bless' },
      { name: 'detect invisible' },
      { name: 'detect magic' },
      { name: 'protection evil' },
    ]);
  });

  it('filters by the query, names that start with it first', () => {
    expect(affectSuggestions(current, ILSABET, 'E').map((s) => s.name)).toEqual([
      'bless',
      'detect invisible',
      'detect magic',
      'protection evil',
    ]);
    expect(affectSuggestions(current, ILSABET, 'det').map((s) => s.name)).toEqual([
      'detect invisible',
      'detect magic',
    ]);
    expect(affectSuggestions(current, ILSABET, 'ion').map((s) => s.name)).toEqual([
      'protection evil',
    ]);
  });

  it('stops at the limit and offers nothing before the server sends affects', () => {
    expect(affectSuggestions(current, [], '', 2).map((s) => s.name)).toEqual(['armor', 'bless']);
    expect(affectSuggestions(null, ILSABET, '')).toEqual([]);
  });
});
