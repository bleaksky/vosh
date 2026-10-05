import { describe, expect, it, vi } from 'vitest';
import type { SessionOutput } from '../ipc/terminal';
import { bandAfterOutput, pinRowAfterOutput, pinRowAfterWrite } from './pinnedPromptStore';

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const bytes = (text: string) => new TextEncoder().encode(text);
const output = (text: string, extra: Partial<SessionOutput> = {}): SessionOutput => ({
  bytes: bytes(text),
  ...extra,
});

// Enter on an empty line echoes nothing only while the row your pinned
// prompt held is still where the next thing lands. A prompt Vosh does not
// read, such as the pager's [Hit Return to continue], stays in the text,
// and Enter there has to end its row as it always did.
describe('the pinned prompt row the page tracks', () => {
  it('opens and closes as each payload says', () => {
    const pin = new Uint8Array(bytes('<1020>'));
    expect(pinRowAfterOutput(false, output('room', { pin, pinRow: true }))).toBe(true);
    expect(pinRowAfterOutput(true, output('', { pin, pinRow: false }))).toBe(false);
  });

  it('closes when text lands from a payload that says nothing', () => {
    // The pager arrives: text Vosh does not read as your prompt.
    expect(pinRowAfterOutput(true, output('Line one.\r\n[Hit Return to continue]'))).toBe(false);
    // A framed echo from outside a read closes it too.
    expect(pinRowAfterOutput(true, output('\r\nTICK IN 5s\r\n'))).toBe(false);
    // Colors alone, or nothing at all, leave it open.
    expect(pinRowAfterOutput(true, output('\x1b[0m'))).toBe(true);
    expect(pinRowAfterOutput(true, output(''))).toBe(true);
    // A fresh replace lands at the cursor too.
    const replace = { gen: 3, bytes: bytes('partial'), fresh: true };
    expect(pinRowAfterOutput(true, output('', { replace }))).toBe(false);
  });

  it('closes when the page writes to the terminal', () => {
    expect(pinRowAfterWrite(true, 'look\r\n')).toBe(false);
    expect(pinRowAfterWrite(true, '\r\n\x1b[31m[Not connected]\x1b[0m\r\n')).toBe(false);
    expect(pinRowAfterWrite(false, 'look\r\n')).toBe(false);
    expect(pinRowAfterWrite(true, '')).toBe(true);
  });
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
