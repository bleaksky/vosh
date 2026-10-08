import { describe, expect, it } from 'vitest';
import { macroClashNote } from './macroClash';

describe('macroClashNote', () => {
  it('says what a session key does elsewhere, and that the macro keeps it here', () => {
    expect(macroClashNote('Meta+1', true)).toBe(
      '⌘1 also goes to your first session. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Meta+9', true)).toBe(
      '⌘9 also goes to your ninth session. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Meta+T', true)).toBe(
      '⌘T also opens a new session. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Meta+W', true)).toBe(
      '⌘W also closes the session in front. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Shift+Meta+W', true)).toBe(
      '⇧⌘W also closes the window. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Shift+Meta+}', true)).toBe(
      '⇧⌘] also goes to the next session. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Shift+Meta+{', true)).toBe(
      '⇧⌘[ also goes to the previous session. In sessions on this profile it runs this macro.',
    );
  });

  it('says which Settings page a Settings key opens', () => {
    expect(macroClashNote('Shift+Meta+1', true)).toBe(
      '⇧⌘1 also opens Triggers in Settings. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Ctrl+Shift+@', false)).toBe(
      'Ctrl+Shift+2 also opens Aliases in Settings. In sessions on this profile it runs this macro.',
    );
  });

  it('names the keys with Ctrl on Windows and Linux', () => {
    expect(macroClashNote('Ctrl+2', false)).toBe(
      'Ctrl+2 also goes to your second session. In sessions on this profile it runs this macro.',
    );
    expect(macroClashNote('Ctrl+Shift+]', false)).toBe(
      'Ctrl+Shift+] also goes to the next session. In sessions on this profile it runs this macro.',
    );
  });

  it('says nothing for a key no session key shares', () => {
    expect(macroClashNote('F1', true)).toBeNull();
    expect(macroClashNote('Meta+K', true)).toBeNull();
    expect(macroClashNote('Ctrl+1', true)).toBeNull();
    expect(macroClashNote('', true)).toBeNull();
  });
});
