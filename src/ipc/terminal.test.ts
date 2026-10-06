import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { decodeOutputPayload, terminalLocalWrite } from './terminal';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

describe('a session output payload', () => {
  const b64 = (text: string) => btoa(text);
  const text = (bytes: Uint8Array | undefined) =>
    bytes === undefined ? undefined : new TextDecoder().decode(bytes);

  it('decodes the bytes alone when nothing is replaced', () => {
    const out = decodeOutputPayload({ b64: b64('You are hungry.\r\n') });
    expect(text(out.bytes)).toBe('You are hungry.\r\n');
    expect(out.replace).toBeUndefined();
    expect(out.restore).toBeUndefined();
  });

  it('decodes a replace and a restore beside the bytes', () => {
    const out = decodeOutputPayload({
      b64: '',
      replace: { gen: 7, b64: b64('\x1b]7717;o;8\x07NEW> '), fresh: true },
      restore: b64('LIVE> '),
    });
    expect(out.bytes).toHaveLength(0);
    expect(out.replace?.gen).toBe(7);
    expect(out.replace?.fresh).toBe(true);
    expect(text(out.replace?.bytes)).toBe('\x1b]7717;o;8\x07NEW> ');
    expect(text(out.restore)).toBe('LIVE> ');
  });

  it('keeps where each piece of your design landed on the band', () => {
    const span = {
      piece: 1,
      row: 1,
      col: 1,
      width: 3,
      fg: { kind: 'default' as const },
      bg: { kind: 'default' as const },
      bold: false,
      italic: false,
      underline: false,
    };
    const out = decodeOutputPayload({ b64: '', pin: b64('tank\r\n<765>'), pin_spans: [span] });
    expect(text(out.pin)).toBe('tank\r\n<765>');
    expect(out.pinSpans).toEqual([span]);
    expect(decodeOutputPayload({ b64: '', pin: b64('[765hp]') }).pinSpans).toBeUndefined();
  });

  it('keeps which output of the prompt stage it is', () => {
    expect(decodeOutputPayload({ b64: b64('<1020> '), id: 42 }).id).toBe(42);
    expect(decodeOutputPayload({ b64: b64('[not connected]\r\n') }).id).toBeUndefined();
  });
});

describe('text the page writes to the terminal itself', () => {
  it('tells the session which output xterm took before it', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await terminalLocalWrite('look\r\n', 42);
    expect(sent).toHaveBeenCalledWith('terminal_local_write', { text: 'look\r\n', after: 42 });
  });

  it('leaves it to the native grid while that renderer shows', async () => {
    const sent = vi.mocked(invoke);
    sent.mockClear();
    await terminalLocalWrite('look\r\n', null);
    expect(sent).toHaveBeenCalledWith('terminal_local_write', { text: 'look\r\n', after: null });
  });
});
