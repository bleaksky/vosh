import { describe, expect, it } from 'vitest';
import { revealLabel } from './revealLabel';

describe('revealLabel', () => {
  it('names the file manager of each platform', () => {
    expect(revealLabel('macos')).toBe('Show in Finder');
    expect(revealLabel('windows')).toBe('Show in Explorer');
    expect(revealLabel('linux')).toBe('Show the folder');
    expect(revealLabel(undefined)).toBe('Show the folder');
  });
});
