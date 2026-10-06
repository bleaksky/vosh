import { describe, expect, it } from 'vitest';
import { countWord, errorText, listJoin, possessive, quoted } from './text';

describe('listJoin', () => {
  it('reads nothing as an empty string and one item as itself', () => {
    expect(listJoin([])).toBe('');
    expect(listJoin(['Orla'])).toBe('Orla');
  });

  it('joins two items with and', () => {
    expect(listJoin(['Maren', 'Orla'])).toBe('Maren and Orla');
  });

  it('puts a serial comma before the last of three or more', () => {
    expect(listJoin(['Tolliver', 'Maren', 'Orla'])).toBe('Tolliver, Maren, and Orla');
    expect(listJoin(['Tolliver', 'Maren', 'Orla', 'the tick'])).toBe(
      'Tolliver, Maren, Orla, and the tick',
    );
  });
});

describe('countWord', () => {
  it('writes two to ten in words and more in digits', () => {
    expect(countWord(2)).toBe('Two');
    expect(countWord(10)).toBe('Ten');
    expect(countWord(11)).toBe('11');
  });
});

describe('possessive', () => {
  it('adds an apostrophe s, after a final s as well', () => {
    expect(possessive('Ilsabet')).toBe("Ilsabet's");
    expect(possessive('Rhys')).toBe("Rhys's");
  });
});

describe('quoted', () => {
  it('sets a name in curly quotes', () => {
    expect(quoted('Maren')).toBe('“Maren”');
  });
});

describe('errorText', () => {
  it('reads an Error by its message and anything else as text, trimmed', () => {
    expect(errorText(new Error(' Orla is not here.\n'))).toBe('Orla is not here.');
    expect(errorText('  no such trigger ')).toBe('no such trigger');
    expect(errorText(42)).toBe('42');
  });
});
