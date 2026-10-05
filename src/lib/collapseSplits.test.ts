import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import splitsB64 from '../../fixtures/collapse/splits.b64?raw';
import { OutputShaper } from './outputShaper';
import { decodeOutputPayload, type OutputPayload } from '../ipc/terminal';
import { RegionWriter } from './terminalRegion';

// Collapse repeated lines, with the session's own payloads for pulses
// and a fight with your prompt pinned, the pulses with it in the text
// too, and lines with no prompt, as one read and as two cut at every
// place, each after the login. Then scenes: runs whose line ends on a
// background across pinned pulses, your echo landing before the session
// heard of it, and a pane that loads your scrollback during a run.
// src-tauri/src/session/tests/collapse.rs writes them from the real
// session steps, and holds the stored file to what the session sends now.
// Each is replayed through the same decode, word wrap and writer
// Terminal.tsx uses, into a real xterm, and every screen has to be the
// native grid's screen, each run shown once with its count.

interface Stream {
  name: string;
  show: string;
  screens: Record<string, string[]>;
  login: OutputPayload[];
  splits: OutputPayload[][];
}

interface Scene {
  name: string;
  cols: number;
  /** What the terminal loads before the payloads, as the history of the
   *  split loads your scrollback. */
  load: string | null;
  payloads: OutputPayload[];
  /** Your echo lands before the payload at `before`. */
  before: number | null;
  echo: string | null;
  screen: string[];
}

interface Splits {
  streams: Stream[];
  scenes: Scene[];
}

async function loadSplits(): Promise<Splits> {
  const bin = atob(splitsB64.replace(/\s+/g, ''));
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
  const text = await new Response(stream).text();
  return JSON.parse(text) as Splits;
}

async function loadStreams(): Promise<Stream[]> {
  return (await loadSplits()).streams;
}

/** Play `scene` into a new xterm as Terminal.tsx would, and hand back the
 *  terminal and its screen. */
async function play(scene: Scene): Promise<{ term: Terminal; rows: string[] }> {
  const term = new Terminal({
    cols: scene.cols,
    rows: 20,
    scrollback: 100,
    allowProposedApi: true,
  });
  const writer = new RegionWriter(term);
  const shaper = new OutputShaper(term.cols);
  if (scene.load !== null) writer.local(scene.load);
  scene.payloads.forEach((payload, i) => {
    if (scene.before === i && scene.echo !== null) writer.local(scene.echo);
    const { output } = shaper.shape(decodeOutputPayload(payload));
    if (output) writer.output(output);
  });
  await new Promise<void>((resolve) => writer.whenParsed(resolve));
  writer.dispose();
  return { term, rows: screen(term) };
}

/** The cell at `x` on screen row `y`. */
function cellAt(term: Terminal, y: number, x: number) {
  const buffer = term.buffer.active;
  const cell = buffer.getLine(buffer.baseY + y)?.getCell(x);
  if (!cell) throw new Error(`no cell at ${x}, ${y}`);
  return cell;
}

/** Every cell from row `from` down is on the default background. */
function plainBelow(term: Terminal, from: number): void {
  for (let y = from; y < term.rows; y++) {
    for (let x = 0; x < term.cols; x++) {
      expect(cellAt(term, y, x).isBgDefault(), `row ${y}, column ${x}`).toBe(true);
    }
  }
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

describe('repeated lines, replayed into xterm at every split', () => {
  it('shows each run once with its count, as the native grid does, 40 and 12 wide', async () => {
    const streams = await loadStreams();
    expect(streams.map((s) => `${s.name} ${s.show}`)).toContain('compact pinned');
    // The runs the grid shows: four dodges, three of the long line, which
    // wraps at 40, and the parry once.
    const compact = streams.find((s) => s.name === 'compact' && s.show === 'pinned');
    expect(compact?.screens['40'].slice(-4)).toEqual([
      "(4) You dodge Quenby's attack.",
      "(3) You dodge Quenby's attack and",
      'redirect the momentum!',
      "You parry Quenby's attack.",
    ]);
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
              got[i] = await replay(term, [...stream.login, ...stream.splits[i]]);
            }
            term.dispose();
          }),
        );
        for (const [i, rows] of got.entries()) {
          expect(rows, `${stream.name} ${stream.show} ${cols} wide, split ${i}`).toEqual(want);
          replays++;
        }
      }
    }
    expect(replays).toBeGreaterThan(1000);
  }, 120_000);
});

describe('repeated lines in scenes the stream replays leave out', () => {
  const dodge = "You dodge Quenby's attack.";
  const parry = "You parry Quenby's attack.";

  async function scene(name: string): Promise<{ term: Terminal; rows: string[]; want: string[] }> {
    const found = (await loadSplits()).scenes.find((s) => s.name === name);
    if (!found) throw new Error(`no scene ${name}`);
    const { term, rows } = await play(found);
    return { term, rows, want: found.screen };
  }

  it('leaves every cell but the colored ones plain as a run crosses pinned pulses', async () => {
    const { term, rows, want } = await scene('blue');
    expect(rows).toEqual(want);
    const run = rows.indexOf(`(2) ${dodge}`);
    expect(rows[run + 1]).toBe(parry);
    for (const [y, text] of [
      [run, `(2) ${dodge}`],
      [run + 1, parry],
    ] as const) {
      for (let x = 0; x < term.cols; x++) {
        const cell = cellAt(term, y, x);
        const blue = x >= text.length - 7 && x < text.length;
        if (blue) expect(cell.isBgPalette() && cell.getBgColor(), `row ${y}, column ${x}`).toBe(4);
        else expect(cell.isBgDefault(), `row ${y}, column ${x}`).toBe(true);
      }
    }
    plainBelow(term, run + 2);
    term.dispose();
  });

  it('keeps the count on the wash and the rows below plain', async () => {
    const { term, rows, want } = await scene('wash');
    expect(rows).toEqual(want);
    const run = rows.indexOf(`(3) ${dodge}`);
    const first = cellAt(term, run, 0);
    expect(first.getChars()).toBe('(');
    expect(first.isBgRGB() && first.getBgColor()).toBe(0x330000);
    for (let x = `(3) ${dodge}`.length; x < term.cols; x++) {
      expect(cellAt(term, run, x).isBgDefault(), `column ${x}`).toBe(true);
    }
    plainBelow(term, run + 1);
    term.dispose();
  });

  it('writes a run your echo landed before on a new row that still ends', async () => {
    const { term, rows, want } = await scene('echo');
    expect(rows).toEqual(want);
    expect(rows.slice(-4)).toEqual([dodge, 'kill guard', `(2) ${dodge}`, parry]);
    const at = rows.indexOf(parry);
    expect(cellAt(term, at, 0).isFgDefault()).toBe(true);
    term.dispose();
  });

  it('goes on in place in a pane that loaded your scrollback during the run', async () => {
    const { term, rows, want } = await scene('pane');
    expect(rows).toEqual(want);
    expect(rows.slice(-2)).toEqual([`(5) ${dodge}`, parry]);
    term.dispose();
  });
});
