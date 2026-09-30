import { describe, expect, it } from 'vitest';
import { Terminal } from '@xterm/xterm';
import {
  eraseBack,
  lastMark,
  RegionWriter,
  type RegionOutput,
  type RegionTerminal,
} from './terminalRegion';

// The region rules against a real xterm with no page around it. Writes
// go in back to back, as session events and your typed echo do, before
// xterm has parsed any of them.

const mark = (gen: number) => `\x1b]7717;o;${gen}\x07`;

function setup(cols = 40, rows = 10) {
  const term = new Terminal({ cols, rows, scrollback: 100, allowProposedApi: true });
  const writer = new RegionWriter(term);
  return { term, writer };
}

/** Resolve once xterm has parsed everything the writer took so far. */
function parsed(writer: RegionWriter): Promise<void> {
  return new Promise((resolve) => writer.whenParsed(resolve));
}

/** Like `parsed`, but gives up after a while, for a writer that may
 *  have stopped for good. */
function settled(writer: RegionWriter): Promise<void> {
  return Promise.race([parsed(writer), new Promise<void>((resolve) => setTimeout(resolve, 200))]);
}

/** The screen's rows, trailing blanks trimmed, up to the last row that
 *  shows anything. */
function screen(term: Terminal): string[] {
  const buffer = term.buffer.active;
  const rows: string[] = [];
  for (let y = 0; y < term.rows; y++) {
    rows.push(buffer.getLine(buffer.baseY + y)?.translateToString(true) ?? '');
  }
  while (rows.length > 0 && rows[rows.length - 1] === '') rows.pop();
  return rows;
}

function replace(gen: number, text: string, fresh = false): RegionOutput {
  return { text: '', replace: { gen, text, fresh } };
}

describe('lastMark and eraseBack', () => {
  it('reads the generation of the last mark in the text', () => {
    expect(lastMark('plain')).toBeNull();
    expect(lastMark(`a${mark(3)}b${mark(12)}c`)).toBe(12);
    expect(lastMark('\x1b]7717;o;\x07')).toBeNull();
  });

  it('goes back up to the region start and erases below it', () => {
    expect(eraseBack(0, 0)).toBe('\r\x1b[0J');
    expect(eraseBack(2, 7)).toBe('\r\x1b[2A\x1b[7C\x1b[0J');
  });
});

describe('RegionWriter', () => {
  const prompts = [
    { name: 'a one row prompt', design: '<1020hp 800m> ', rows: ['<1020hp 800m> '] },
    {
      name: 'a two row prompt',
      design: 'Tank 100%\r\n<1020hp 800m> ',
      rows: ['Tank 100%', '<1020hp 800m> '],
    },
  ];

  for (const { name, design, rows } of prompts) {
    it(`drops a repaint that reaches xterm after your echo, with ${name}`, async () => {
      const { term, writer } = setup();
      writer.output({ text: `You are hungry.\r\n${mark(1)}${design}` });
      writer.local('look\r\n');
      writer.output(replace(1, `${mark(2)}NEW> `));
      await parsed(writer);
      const last = rows.length - 1;
      expect(screen(term)).toEqual([
        'You are hungry.',
        ...rows.slice(0, last),
        `${rows[last]}look`.trimEnd(),
      ]);
      // The echo is never erased and the prompt never shows twice.
      expect(
        screen(term)
          .join('\n')
          .match(/1020hp/g),
      ).toHaveLength(1);
      expect(screen(term).join('\n')).not.toContain('NEW');
    });

    it(`puts your echo after a repaint that came first, with ${name}`, async () => {
      const { term, writer } = setup();
      writer.output({ text: `You are hungry.\r\n${mark(1)}${design}` });
      writer.output(replace(1, `${mark(2)}NEW> `));
      writer.local('look\r\n');
      await parsed(writer);
      expect(screen(term)).toEqual(['You are hungry.', 'NEW> look']);
    });
  }

  it('replaces a region that ends with its line end in place', async () => {
    // A prompt whole before the line end that came after it keeps that
    // line end in its region.
    const { term, writer } = setup();
    writer.output({ text: `You are hungry.\r\n${mark(1)}<1020>\r\n` });
    writer.output(replace(1, `${mark(2)}<1020hp 800m> \r\n`));
    writer.local('look\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', '<1020hp 800m> ', 'look']);
  });

  it('counts the rows xterm wrapped the region into', async () => {
    const { term, writer } = setup(12);
    writer.output({ text: `before\r\n${mark(1)}[1020/1020hp 800/800mn 930/930mv]` });
    await parsed(writer);
    expect(screen(term)).toHaveLength(4);
    writer.output(replace(1, `${mark(2)}NEW`));
    writer.local('look\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual(['before', 'NEWlook']);
  });

  it('drops a replace after other output, and writes a fresh one on a new row', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}abc` });
    writer.output({ text: 'xyz' });
    writer.output(replace(1, 'dropped'));
    await parsed(writer);
    expect(screen(term)).toEqual(['abcxyz']);
    writer.output(replace(1, 'abcdef\r\n', true));
    await parsed(writer);
    expect(screen(term)).toEqual(['abcxyz', 'abcdef']);
    // At the start of a row a fresh replace writes right there.
    writer.output({ text: `${mark(2)}You are hun` });
    writer.local('look\r\n');
    writer.output(replace(2, 'You are hungry.\r\n', true));
    await parsed(writer);
    expect(screen(term)).toEqual(['abcxyz', 'abcdef', 'You are hunlook', 'You are hungry.']);
  });

  it('replaces a painted partial with the line that completes it', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}You are hun` });
    writer.output({
      text: 'You feel better.\r\n',
      replace: { gen: 1, text: 'You are hungry.\r\n', fresh: true },
    });
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', 'You feel better.']);
    // The completed line has no mark, so nothing stays open.
    writer.output(replace(1, 'again'));
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', 'You feel better.']);
  });

  it('keeps what came before a region that starts mid row', async () => {
    const { term, writer } = setup();
    writer.output({ text: '<10hp> ' });
    writer.output({ text: `${mark(1)}You are hun` });
    writer.output(replace(1, 'You are hungry.\r\n', true));
    await parsed(writer);
    expect(screen(term)).toEqual(['<10hp> You are hungry.']);
  });

  it('starts a region at the next row when its mark came after a full row', async () => {
    const { term, writer } = setup(10);
    writer.local('0123456789');
    writer.output({ text: `${mark(1)}PROMPT` });
    writer.output(replace(1, `${mark(2)}NEW`));
    await parsed(writer);
    expect(screen(term)).toEqual(['0123456789', 'NEW']);
  });

  it('counts a region whose start scrolled above the screen as closed', async () => {
    const { term, writer } = setup(40, 3);
    writer.output({ text: `${mark(1)}one\r\ntwo\r\nthree\r\nfour` });
    writer.output(replace(1, 'dropped'));
    await parsed(writer);
    expect(screen(term)).toEqual(['two', 'three', 'four']);
    writer.output(replace(1, 'fresh', true));
    await parsed(writer);
    expect(screen(term)).toEqual(['three', 'four', 'fresh']);
  });

  it('puts the live render back before a local write', async () => {
    const { term, writer } = setup();
    writer.output({ text: `You are hungry.\r\n${mark(1)}PREVIEW> `, restore: 'LIVE> ' });
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', 'PREVIEW> ']);
    writer.local('look\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', 'LIVE> look']);
  });

  it('puts the live render back before session output too', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}PREVIEW> `, restore: 'LIVE> ' });
    writer.output({ text: '\r\nYou flee!\r\n' });
    await parsed(writer);
    expect(screen(term)).toEqual(['LIVE> ', 'You flee!']);
  });

  it('lets a replace of the region, then text, go in that order over a restore', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}PREVIEW> `, restore: 'LIVE> ' });
    writer.output({
      text: '\r\nYou flee!\r\n',
      replace: { gen: 1, text: `${mark(2)}NEW> `, fresh: false },
    });
    await parsed(writer);
    expect(screen(term)).toEqual(['NEW> ', 'You flee!']);
  });

  it('lets a replace of the region itself take the place of its restore', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}PREVIEW> `, restore: 'LIVE> ' });
    writer.output({ ...replace(1, `${mark(2)}OTHER> `), restore: 'LIVE2> ' });
    writer.output(replace(2, `${mark(3)}PLAIN> `));
    writer.local('look\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual(['PLAIN> look']);
  });

  it('writes nothing more once disposed, even a replace that waited', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}PROMPT> ` });
    writer.output(replace(1, `${mark(2)}NEW> `));
    writer.local('look\r\n');
    writer.dispose();
    writer.local('after\r\n');
    await new Promise((resolve) => term.write('', () => resolve(undefined)));
    expect(screen(term)).toEqual(['PROMPT> ']);
  });

  it('keeps writing after a resize lands while a replace waits', async () => {
    // xterm parses everything it holds when it resizes. The replace
    // still goes in, and what came after it still reaches the screen.
    const { term, writer } = setup();
    writer.output({ text: `You are hungry.\r\n${mark(1)}PROMPT> ` });
    await parsed(writer);
    writer.output(replace(1, `${mark(2)}NEW> `));
    term.resize(30, 10);
    writer.local('look\r\n');
    writer.output({ text: 'You see nothing special.\r\n' });
    await settled(writer);
    expect(screen(term)).toEqual(['You are hungry.', 'NEW> look', 'You see nothing special.']);
  });

  it('replaces a region at the width xterm parsed it at when a resize narrows it', async () => {
    const { term, writer } = setup();
    writer.output({ text: `abcdefghijklmnopqrstuvwxyz${mark(1)}You are hun` });
    writer.output(replace(1, 'You are hungry.\r\n', true));
    term.resize(12, 10);
    await settled(writer);
    expect(screen(term).join('')).toBe('abcdefghijklmnopqrstuvwxyzYou are hungry.');
  });

  it('runs a parse callback when a resize lands before xterm parses', async () => {
    const { term, writer } = setup();
    writer.local('restored scrollback\r\n');
    const ran: string[] = [];
    writer.whenParsed(() => ran.push(screen(term).join('|')));
    term.resize(30, 10);
    await settled(writer);
    expect(ran).toEqual(['restored scrollback']);
  });

  it('waits once when xterm calls back twice for the same write', async () => {
    // xterm's flush before a resize can parse writes it already parsed,
    // and call their callbacks again.
    const writes: string[] = [];
    const callbacks: (() => void)[] = [];
    const term: RegionTerminal = {
      cols: 40,
      buffer: { active: { cursorX: 0, cursorY: 0, baseY: 0 } },
      parser: { registerOscHandler: () => ({ dispose() {} }) },
      write(data, callback) {
        if (typeof data === 'string') writes.push(data);
        if (callback) callbacks.push(callback);
      },
      registerMarker: () => undefined,
    };
    const parseTwice = () => {
      const pending = callbacks.splice(0);
      for (const callback of [...pending, ...pending]) callback();
    };
    const writer = new RegionWriter(term);
    let ran = 0;
    writer.output({ text: 'You are hun' });
    writer.output(replace(1, 'You are hungry.\r\n', true));
    writer.local('look\r\n');
    writer.whenParsed(() => ran++);
    parseTwice();
    parseTwice();
    await Promise.resolve();
    expect(writes).toEqual(['You are hun', 'You are hungry.\r\n', 'look\r\n']);
    expect(ran).toBe(1);
  });

  it('keeps the order of writes that arrive while a replace waits', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}PROMPT> ` });
    writer.output(replace(1, `${mark(2)}NEW> `));
    writer.local('look\r\n');
    writer.output({ text: 'You see nothing special.\r\n' });
    writer.output({ text: `${mark(3)}NEXT> ` });
    writer.output(replace(3, `${mark(4)}LAST> `));
    await parsed(writer);
    expect(screen(term)).toEqual(['NEW> look', 'You see nothing special.', 'LAST> ']);
  });
});
