import { describe, expect, it } from 'vitest';
import { profileDisplayName, profilePossessive } from './profileLabel';

describe('profileDisplayName', () => {
  it('reads the reserved default profile as Default', () => {
    expect(profileDisplayName('default')).toBe('Default');
  });

  it('keeps every other name as it is', () => {
    expect(profileDisplayName('Ilsabet')).toBe('Ilsabet');
    expect(profileDisplayName('aabahran-ilsabet')).toBe('aabahran-ilsabet');
    expect(profileDisplayName('Default')).toBe('Default');
  });
});

describe('profilePossessive', () => {
  it('names whose panel a row changes', () => {
    expect(profilePossessive('Ilsabet')).toBe("Ilsabet's");
    expect(profilePossessive('default')).toBe("Default's");
  });
});
