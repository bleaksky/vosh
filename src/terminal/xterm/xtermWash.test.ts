import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import fixture from '../../../fixtures/wash/fields.json';
import { OutputShaper } from '../outputShaper';
import { decodeOutputPayload } from '../../ipc/terminal';
import { refillsWashes, WashPainter, washFields } from './xtermWash';

// Washed lines on xterm paint the field the native renderer paints, in
// the theme's colors (src-tauri/src/native/gpu/frame.rs). The lines are
// the trigger engine's own bytes, from wash_wraps_whole_line and
// wash_uses_explicit_bg_over_fg in crates/automation/src/trigger/engine.rs.

const ember = fixture.cases[0];
const fields = washFields(ember.palette, ember.ground);
/** The field of a yellow wash and of a red one on Obsidian Ember. */
const YELLOW = 0x2a2314;
const RED = 0x2a1815;

const SANCTUARY =
  '\x1b[33;48;2;51;51;0mYour \x1b[33msanctuary\x1b[0m\x1b[33;48;2;51;51;0m flickers and fades.\x1b[0m\r\n';
const DANGER =
  '\x1b[37;48;2;51;0;0m\x1b[37;41mDANGER\x1b[0m\x1b[37;48;2;51;0;0m close behind you\x1b[0m\r\n';

function paint(text: string): string {
  return WashPainter.whole(text, fields);
}

/** A real xterm `cols` wide with `text` written into it. */
async function screen(text: string, cols: number): Promise<Terminal> {
  const term = new Terminal({ cols, rows: 6, allowProposedApi: true });
  await new Promise<void>((done) => term.write(text, done));
  return term;
}

/** Each cell's background on row `y`: its RGB, or null for any other. */
function grounds(term: Terminal, y: number): (number | null)[] {
  const line = term.buffer.active.getLine(y);
  const out: (number | null)[] = [];
  for (let x = 0; x < term.cols; x++) {
    const cell = line?.getCell(x);
    out.push(cell?.isBgRGB() ? cell.getBgColor() : null);
  }
  return out;
}

describe('washFields', () => {
  it('gives the field the native renderer paints for every mark', () => {
    for (const c of fixture.cases) {
      const got = washFields(c.palette, c.ground);
      expect([...got], c.name).toEqual(c.washes.map((w) => [w.tint, w.field]));
    }
  });
});

describe('WashPainter', () => {
  it('paints a washed line in the field, out to the last column', async () => {
    const out = paint(SANCTUARY);
    expect(out).not.toContain('48;2;51;51;0');
    expect(out).toContain('\x1b[33;48;2;42;35;20mYour ');
    expect(out.endsWith('\x1b[48;2;42;35;20m\x1b[K\x1b[49m\r\n')).toBe(true);
    const term = await screen(out, 50);
    expect(grounds(term, 0)).toEqual(Array(50).fill(YELLOW));
    // The text keeps the mark color, which the theme resolves.
    const y = term.buffer.active.getLine(0)!.getCell(0)!;
    expect(y.isFgPalette() && y.getFgColor()).toBe(3);
    // The next line is clean.
    expect(grounds(term, 1)).toEqual(Array(50).fill(null));
    term.dispose();
  });

  it('leaves a row whose first cell takes another ground unwashed, as native does', async () => {
    // The explicit red under DANGER sits on the first cell, so the native
    // renderer reads no signal there and paints the rest on the ground.
    const out = paint(DANGER);
    expect(out).not.toContain('48;2;51;0;0');
    expect(out).not.toContain('\x1b[K');
    const term = await screen(out, 40);
    const line = term.buffer.active.getLine(0)!;
    expect(line.getCell(0)!.isBgPalette() && line.getCell(0)!.getBgColor()).toBe(1);
    expect(grounds(term, 0).slice(6)).toEqual(Array(34).fill(null));
    expect(line.getCell(6)!.isBgDefault()).toBe(true);
    term.dispose();
  });

  it('paints the field of the mark a red wash opens with', async () => {
    const out = paint('\x1b[37;48;2;51;0;0mDANGER close behind you\x1b[0m\r\n');
    const term = await screen(out, 40);
    expect(grounds(term, 0)).toEqual(Array(40).fill(RED));
    term.dispose();
  });

  it('leaves a truecolor ground that is no signal alone', () => {
    const line = '\x1b[48;2;10;20;30mThe sky is dark.\x1b[0m\r\n';
    expect(paint(line)).toBe(line);
    const colon = '\x1b[48:2::10:20:30mThe sky is dark.\x1b[0m\r\n';
    expect(paint(colon)).toBe(colon);
  });

  it('reads the colon form of the signal too', async () => {
    const out = paint('\x1b[48:2::51:51:0mYour sanctuary flickers and fades.\x1b[0m\r\n');
    expect(out).not.toContain('51:51:0');
    const term = await screen(out, 40);
    expect(grounds(term, 0)).toEqual(Array(40).fill(YELLOW));
    term.dispose();
  });

  it('paints the same when an output splits the line anywhere', () => {
    const whole = paint(SANCTUARY);
    for (let at = 1; at < SANCTUARY.length; at++) {
      const painter = new WashPainter(() => fields);
      const out = painter.paint(SANCTUARY.slice(0, at)) + painter.paint(SANCTUARY.slice(at));
      expect(out, `split at ${at}`).toBe(whole);
    }
  });

  it('gives every row of a hard wrapped washed line the field', async () => {
    const shaper = new OutputShaper(16, () => fields);
    const shaped = shaper.shape(decodeOutputPayload({ b64: btoa(SANCTUARY) }));
    const text = shaped.output!.text;
    expect(text.split('\r\n').length).toBeGreaterThan(2);
    const term = await screen(text, 16);
    expect(term.buffer.active.getLine(1)!.translateToString(true)).toBe('flickers and');
    for (let y = 0; y < 3; y++)
      expect(grounds(term, y), `row ${y}`).toEqual(Array(16).fill(YELLOW));
    term.dispose();
  });

  it('paints a row of signal colored blanks as the ground', async () => {
    const out = paint('\x1b[48;2;51;51;0m          \x1b[0m\r\n');
    expect(out).not.toContain('48;2;');
    expect(out).not.toContain('\x1b[K');
    const term = await screen(out, 20);
    expect(grounds(term, 0)).toEqual(Array(20).fill(null));
    term.dispose();
  });

  it('keeps leading blanks on a washed row in the field', async () => {
    const out = paint('\x1b[48;2;51;51;0m  Your sanctuary flickers and fades.\x1b[0m\r\n');
    const term = await screen(out, 40);
    expect(grounds(term, 0)).toEqual(Array(40).fill(YELLOW));
    term.dispose();
  });

  it('erases with the field while the signal is in force', async () => {
    const out = paint('\x1b[48;2;51;51;0m\x1b[2KYour sanctuary flickers and fades.\x1b[0m\r\n');
    const term = await screen(out, 40);
    expect(grounds(term, 0)).toEqual(Array(40).fill(YELLOW));
    term.dispose();
  });
});

describe('refillsWashes', () => {
  const other = washFields(fixture.cases[1].palette, fixture.cases[1].ground);

  it('fills anew once a wash painted and the fields changed', () => {
    expect(refillsWashes(fields, other, true)).toBe(true);
    expect(refillsWashes(new Map(), fields, true)).toBe(true);
  });

  it('keeps the screen when nothing washed', () => {
    expect(refillsWashes(fields, other, false)).toBe(false);
  });

  it('keeps the screen when the fields are the same', () => {
    const same = washFields(ember.palette, ember.ground);
    expect(refillsWashes(fields, same, true)).toBe(false);
  });
});

describe('WashPainter hears its washes', () => {
  it('tells a washed row, and nothing for a plain one', () => {
    let heard = 0;
    WashPainter.whole('The day has begun.\r\n', fields, () => (heard += 1));
    expect(heard).toBe(0);
    WashPainter.whole(SANCTUARY, fields, () => (heard += 1));
    expect(heard).toBeGreaterThan(0);
  });
});
