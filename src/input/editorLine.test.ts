import { describe, expect, it } from 'vitest';
import { WRITING_IDLE } from '../stores/session/writingStore';
import { editorLineOf, heldLine, washPast } from './editorLine';

describe('the command line in the game’s editor', () => {
  it('shows the tick while the editor holds a text Vosh names and the card does not drive it', () => {
    expect(editorLineOf(WRITING_IDLE)).toBeNull();
    const open = { ...WRITING_IDLE, game: 'editor' as const, editor: 'description' as const };
    expect(editorLineOf(open)).toEqual({ kind: 'description', width: 75, helpWidth: true });
    const job = {
      id: 1,
      kind: 'description' as const,
      action: 'read' as const,
      stage: 'closing' as const,
      sent: 0,
      total: 0,
    };
    expect(editorLineOf({ ...open, job })).toBeNull();
  });

  it('washes only what runs past the tick', () => {
    const desc = { kind: 'description' as const, width: 75, helpWidth: true };
    expect(washPast('short', desc, 8)).toBeUndefined();
    expect(washPast('x'.repeat(77), desc, 8)?.backgroundImage).toContain('600px');
  });

  it('says how many sends wait', () => {
    expect(heldLine(1)).toBe('1 send waits');
    expect(heldLine(3)).toBe('3 sends wait');
  });
});
