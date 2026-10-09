import { describe, expect, it } from 'vitest';
import { looksLikeChat } from './chatLine';

describe('looksLikeChat', () => {
  it('knows a chat verb at the start of the line', () => {
    expect(looksLikeChat('say The day has begun.')).toBe(true);
    expect(looksLikeChat("'hello")).toBe(true);
    expect(looksLikeChat('tell Maren hi')).toBe(true);
    expect(looksLikeChat('  ooc back soon')).toBe(true);
  });

  it('leaves game commands and an empty line alone', () => {
    expect(looksLikeChat('look')).toBe(false);
    expect(looksLikeChat('kill orc')).toBe(false);
    expect(looksLikeChat('tell Maren')).toBe(false);
    expect(looksLikeChat('   ')).toBe(false);
  });
});
