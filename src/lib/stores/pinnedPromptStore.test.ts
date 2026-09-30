import { describe, expect, it, vi } from 'vitest';
import type { SessionOutput } from '../session';
import { pinRowAfterOutput, pinRowAfterWrite } from './pinnedPromptStore';

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
