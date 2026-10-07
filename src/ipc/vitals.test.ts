import { describe, expect, it } from 'vitest';
import shared from '../../fixtures/ui-config/vitals-text.txt?raw';
import { VOSH_VITALS_TEXT } from './vitals';

describe('VOSH_VITALS_TEXT', () => {
  it('is the text Rust draws for a profile that sets none', () => {
    // crates/prompt/src/config.rs holds DEFAULT_VITALS_TEXT to the same file.
    expect(VOSH_VITALS_TEXT).toBe(shared);
  });
});
