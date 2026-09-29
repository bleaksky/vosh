import { describe, expect, it } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { parseCharState } from './charStateStore';

describe('parseCharState', () => {
  it('reads your position and spoken language', () => {
    expect(parseCharState(aabahranPacket('char-state.gmcp').data)).toEqual({
      position: 'sitting',
      language: 'common',
    });
  });

  it('reads the mortally wounded position whole', () => {
    expect(parseCharState({ position: 'mortally wounded', language: 'drow' })).toEqual({
      position: 'mortally wounded',
      language: 'drow',
    });
  });

  it('reads an empty language as none', () => {
    expect(parseCharState({ position: 'standing', language: '' })).toEqual({
      position: 'standing',
      language: null,
    });
    expect(parseCharState({})).toEqual({ position: null, language: null });
    expect(parseCharState(null)).toBeNull();
  });
});
