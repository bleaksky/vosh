import { describe, expect, it } from 'vitest';
import { snoopHandoff, type SnoopKey } from './snoopHandoff';

const key = (k: string, over: Partial<SnoopKey> = {}): SnoopKey => ({
  key: k,
  metaKey: false,
  ctrlKey: false,
  ...over,
});

describe('a key in a snoop terminal', () => {
  it('goes back to the command line on Esc and takes the key', () => {
    expect(snoopHandoff(key('Escape'))).toBe('escape');
  });

  it('goes back with any key that types, which lands there', () => {
    for (const k of ['l', 'L', '3', ' ', "'", ';', 'é', 'ø', '😀']) {
      expect(snoopHandoff(key(k))).toBe('type');
    }
  });

  it('leaves Cmd and Ctrl keys to the snoop, so Cmd J, F and C reach it', () => {
    for (const k of ['j', 'f', 'c']) {
      expect(snoopHandoff(key(k, { metaKey: true }))).toBe('stay');
      expect(snoopHandoff(key(k, { ctrlKey: true }))).toBe('stay');
    }
  });

  it('leaves the keys that type nothing', () => {
    for (const k of ['ArrowUp', 'PageDown', 'Shift', 'Enter', 'Tab', 'F1', 'Dead']) {
      expect(snoopHandoff(key(k))).toBe('stay');
    }
  });

  it('leaves a key that composes, so an input method finishes first', () => {
    expect(snoopHandoff(key('a', { isComposing: true }))).toBe('stay');
  });
});
