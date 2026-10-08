import { describe, expect, it } from 'vitest';
import { applicationGuide, applicationOf, CUSTOM_RACE } from './applications';
import { codeSlot } from './gameCodes';
import { hasBeast, keepsCodes, KINDS, writable } from './kinds';

describe('the kinds', () => {
  it('lets each character write the boards their level allows', () => {
    expect(writable(30)).toEqual(['note', 'journal', 'application', 'idea', 'bug', 'typo']);
    expect(writable(53)).toEqual([
      'note',
      'journal',
      'application',
      'idea',
      'bug',
      'typo',
      'news',
      'penalty',
    ]);
    expect(writable(60)).toContain('changes');
  });

  it('gives a werebeast of level 15 its beast, and trust 55 its codes', () => {
    expect(hasBeast('werebeast', 15)).toBe(true);
    expect(hasBeast('werebeast', 14)).toBe(false);
    expect(hasBeast('elf', 30)).toBe(false);
    expect(keepsCodes(55)).toBe(true);
    expect(keepsCodes(54)).toBe(false);
  });

  it('names each palette row for what you do', () => {
    expect(KINDS.bug.palette).toBe('Report a bug…');
    expect(KINDS.note.palette).toBe('Write a note…');
    expect(KINDS.description.palette).toBe('Edit your description…');
  });
});

describe('an application’s guide', () => {
  it('reads the subject the way the game does', () => {
    expect(applicationOf('Psi Application')?.name).toBe('psi');
    expect(applicationOf('psionics app')?.name).toBe('psi');
    expect(applicationOf('Application to Justice')).toBeNull();
    expect(applicationOf('cabal application')?.name).toBe('cabal');
    expect(applicationOf('Custom Warcry Application')?.name).toBe('custom warcry');
  });

  it('lists the words each help asks for with no words it knows, and help qrace for a custom race', () => {
    expect(applicationGuide('A request', false).head).toBe('Applications');
    expect(applicationGuide('Psi Application', true)).toBe(CUSTOM_RACE);
  });
});

describe('the game’s codes', () => {
  it('draws each code in the color the game writes', () => {
    expect(codeSlot('#')).toEqual({ color: 11, bold: true, name: 'bold yellow' });
    expect(codeSlot('5')).toEqual({ color: 5, bold: false, name: 'purple' });
    expect(codeSlot('8')).toEqual({ color: 8, bold: true, name: 'bold black' });
    expect(codeSlot('`')).toBeNull();
  });
});
