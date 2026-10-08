import { describe, expect, it } from 'vitest';
import { WRITING_IDLE } from '../stores/session/writingStore';
import { editorCount, editorLineOf, heldLine, washPast } from './editorLine';

describe('the command line in the game’s editor', () => {
  it('shows the count while the editor holds a text Vosh names and the card does not drive it', () => {
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

  it('counts a tome, a vote, paper and a pet in warn, since no help sets their width', () => {
    for (const kind of ['tome', 'vote', 'paper', 'pet'] as const) {
      const open = { ...WRITING_IDLE, game: 'editor' as const, editor: kind };
      expect(editorLineOf(open)).toEqual({ kind, width: 75, helpWidth: false });
    }
  });

  it('counts the line in danger past a width a help sets, and in warn otherwise', () => {
    const desc = { kind: 'description' as const, width: 75, helpWidth: true };
    const note = { kind: 'note' as const, width: 75, helpWidth: false };
    const long = 'x'.repeat(76);
    expect(editorCount(long, desc)).toEqual({ text: '76 / 75', tone: 'bad' });
    expect(editorCount(long, note)).toEqual({ text: '76 / 75', tone: 'warn' });
    expect(editorCount('.a dot', desc).tone).toBe('warn');
    expect(editorCount('~', desc).tone).toBe('warn');
    expect(editorCount('plain', desc)).toEqual({ text: '5 / 75', tone: '' });
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
