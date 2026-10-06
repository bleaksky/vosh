import { describe, expect, it, vi } from 'vitest';
import type { SessionOutput } from '../../ipc/terminal';
import { bandAfterOutput } from './pinnedPromptStore';

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const bytes = (text: string) => new TextEncoder().encode(text);
const output = (text: string, extra: Partial<SessionOutput> = {}): SessionOutput => ({
  bytes: bytes(text),
  ...extra,
});

// The band keeps the pieces of your design the latest pin carried, so a
// pointer on the dock maps to the piece under it.
describe('the band the page keeps', () => {
  const span = (piece: number, row: number, col: number, width: number) => ({
    piece,
    row,
    col,
    width,
    fg: { kind: 'default' as const },
    bg: { kind: 'default' as const },
    bold: false,
    italic: false,
    underline: false,
  });
  const decoder = new TextDecoder();

  it('takes the text and pieces of each pin and leaves the band alone without one', () => {
    const spans = [span(0, 1, 0, 1), span(1, 1, 1, 3)];
    const pinned = bandAfterOutput(
      null,
      output('', { pin: bytes('tank\r\n<765>'), pinSpans: spans }),
      decoder,
    );
    expect(pinned).toEqual({ text: 'tank\r\n<765>', spans });
    expect(bandAfterOutput(pinned, output('room\r\n'), decoder)).toBe(pinned);
  });

  it('keeps no pieces for a band that shows no design, and an empty pin clears it', () => {
    const shown = bandAfterOutput(null, output('', { pin: bytes('[765hp]') }), decoder);
    expect(shown).toEqual({ text: '[765hp]', spans: [] });
    expect(bandAfterOutput(shown, output('', { pin: bytes('') }), decoder)).toBeNull();
  });
});
