import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import splitsB64 from '../../fixtures/prompt/aabahran/pinned/splits.b64?raw';
import { OutputShaper } from './outputShaper';
import { decodeOutputPayload, type OutputPayload } from './session';
import { RegionWriter } from './terminalRegion';

// The session's own payloads with your prompt pinned, for every wire
// fixture and a few pulses written back to back, as one read and as two
// cut at every place. src-tauri/src/session/tests/show.rs writes them from
// the real session steps, and holds the stored file to what the session
// sends now. Each is replayed through the same decode, word wrap and
// writer Terminal.tsx uses, into a real xterm, and every screen has to
// be the native grid's screen of one read.

interface Stream {
  name: string;
  draw: boolean;
  screens: Record<string, string[]>;
  splits: OutputPayload[][];
}

async function loadStreams(): Promise<Stream[]> {
  const bin = atob(splitsB64.replace(/\s+/g, ''));
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
  const text = await new Response(stream).text();
  return (JSON.parse(text) as { streams: Stream[] }).streams;
}

/** The screen's rows, trailing blanks and spaces trimmed as the native
 *  grid test trims them, up to the last row that shows anything. */
function screen(term: Terminal): string[] {
  const buffer = term.buffer.active;
  const rows: string[] = [];
  for (let y = 0; y < term.rows; y++) {
    rows.push((buffer.getLine(buffer.baseY + y)?.translateToString(true) ?? '').trimEnd());
  }
  while (rows.length > 0 && rows[rows.length - 1] === '') rows.pop();
  return rows;
}

/** Replay `payloads` into `term`, reset to a blank screen with a new
 *  writer and word wrap, and return its screen. */
async function replay(term: Terminal, payloads: OutputPayload[]): Promise<string[]> {
  term.reset();
  const writer = new RegionWriter(term);
  const shaper = new OutputShaper(term.cols);
  for (const payload of payloads) {
    const { output } = shaper.shape(decodeOutputPayload(payload));
    if (output) writer.output(output);
  }
  await new Promise<void>((resolve) => writer.whenParsed(resolve));
  const rows = screen(term);
  writer.dispose();
  return rows;
}

describe('pinned prompts, replayed into xterm at every split', () => {
  it('shows the native grid screen at every cut, 40 and 12 wide', async () => {
    const streams = await loadStreams();
    expect(streams.map((s) => s.name)).toContain('three-fight-pulses');
    let replays = 0;
    for (const stream of streams) {
      for (const cols of [40, 12]) {
        const want = stream.screens[String(cols)];
        // xterm parses on a timer, so the replays run side by side, each
        // lane on its own terminal.
        const lanes = 16;
        const got: string[][] = [];
        await Promise.all(
          Array.from({ length: lanes }, async (_, lane) => {
            const term = new Terminal({ cols, rows: 60, scrollback: 100, allowProposedApi: true });
            for (let i = lane; i < stream.splits.length; i += lanes) {
              got[i] = await replay(term, stream.splits[i]);
            }
            term.dispose();
          }),
        );
        for (const [i, rows] of got.entries()) {
          expect(rows, `${stream.name} draw ${stream.draw} ${cols} wide, split ${i}`).toEqual(want);
          replays++;
        }
      }
    }
    expect(replays).toBeGreaterThan(1000);
  }, 120_000);
});
