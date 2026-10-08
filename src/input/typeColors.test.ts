import { describe, expect, it } from 'vitest';
import { kindOf, typeSpans } from './typeColors';

// The first word decides, and only what Vosh knows counts.
const words = { aliases: ['eb'], commands: ['alias', 'walk', 'help'] };
const runs = (value: string) => typeSpans(value, words).map((s) => [s.text, s.kind]);

describe('kindOf', () => {
  it('leaves a game command plain', () => {
    expect(kindOf('look', words)).toBeNull();
    expect(kindOf('', words)).toBeNull();
  });

  it('knows your aliases by their exact name', () => {
    expect(kindOf('eb', words)).toBe('alias');
    expect(kindOf('eb orc', words)).toBe('alias');
    expect(kindOf('EB', words)).toBeNull();
  });

  it('knows the # commands Vosh runs, and marks any other', () => {
    expect(kindOf('#walk 3n2e', words)).toBe('hash');
    expect(kindOf('#walkies', words)).toBe('unknown');
    expect(kindOf('#', words)).toBe('unknown');
  });

  it('reads a # command the way the dispatcher does', () => {
    expect(kindOf('# alias kk kick', words)).toBe('hash');
    expect(kindOf('#walk;look', words)).toBe('hash');
    expect(kindOf('#walk 3n;look', words)).toBe('hash');
    expect(kindOf('#walk3n', words)).toBe('unknown');
  });

  it('knows a chat line by the list spell check reads', () => {
    expect(kindOf('say The day has begun.', words)).toBe('chat');
    expect(kindOf("'hello", words)).toBe('chat');
    expect(kindOf('tell Maren hi', words)).toBe('chat');
  });
});

describe('typeSpans', () => {
  it('colors the first word of an alias or a # command only', () => {
    expect(runs('eb')).toEqual([['eb', 'alias']]);
    expect(runs('#walk 3n2e')).toEqual([
      ['#walk', 'hash'],
      [' 3n2e', null],
    ]);
    expect(runs('  #walkies now')).toEqual([
      ['  ', null],
      ['#walkies', 'unknown'],
      [' now', null],
    ]);
    expect(runs('# alias kk kick')).toEqual([
      ['# alias', 'hash'],
      [' kk kick', null],
    ]);
    expect(runs('#walk;look')).toEqual([
      ['#walk', 'hash'],
      [';look', null],
    ]);
  });

  it('colors a chat line whole and leaves a plain line alone', () => {
    expect(runs('say The day has begun.')).toEqual([['say The day has begun.', 'chat']]);
    expect(runs('look')).toEqual([['look', null]]);
  });

  it('judges each line of a compose on its own', () => {
    expect(runs('eb orc\nlook\ntell Orla on my way')).toEqual([
      ['eb', 'alias'],
      [' orc\nlook\n', null],
      ['tell Orla on my way', 'chat'],
    ]);
  });
});
