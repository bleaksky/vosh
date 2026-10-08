import { afterEach, describe, expect, it, vi } from 'vitest';
import { Terminal } from '@xterm/xterm';
import { ECHO_CARET } from '../input/maskedInput';
import { LiftTracker } from './xterm/liftBands';
import {
  closePinRow,
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
const liftStart = (id: number) => `\x1b]7717;l;${id}\x07`;
const liftEnd = (id: number) => `\x1b]7717;e;${id}\x07`;

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
    expect(eraseBack(0, 0)).toBe('\r\x1b[49m\x1b[0J');
    expect(eraseBack(2, 7)).toBe('\r\x1b[2A\x1b[7C\x1b[49m\x1b[0J');
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

  it('colors your target again in the people of a look when their packet comes a read later', async () => {
    const { term, writer } = setup(60);
    const villager = 'A Blackwatch villager scurries about.';
    const resting = 'Tolliver is resting here.';
    writer.output({ text: `[Exits: south]\r\n${mark(1)}${villager}\r\n${resting}\r\n` });
    writer.output(replace(1, `${villager}\r\n\x1b[91m${resting}\x1b[0m\r\n`));
    writer.output({ text: 'Maren arrives from the south.\r\n' });
    await parsed(writer);
    expect(screen(term)).toEqual([
      '[Exits: south]',
      villager,
      resting,
      'Maren arrives from the south.',
    ]);
    const buffer = term.buffer.active;
    expect(
      buffer
        .getLine(buffer.baseY + 1)
        ?.getCell(0)
        ?.isFgDefault(),
    ).toBeTruthy();
    expect(
      buffer
        .getLine(buffer.baseY + 2)
        ?.getCell(0)
        ?.getFgColor(),
    ).toBe(9);
    // Once anything landed after the people, they stay as they show.
    writer.output(replace(1, `\x1b[91m${villager}\x1b[0m\r\n${resting}\r\n`));
    await parsed(writer);
    expect(
      buffer
        .getLine(buffer.baseY + 1)
        ?.getCell(0)
        ?.isFgDefault(),
    ).toBeTruthy();
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
      buffer: { active: { cursorX: 0, cursorY: 0, baseY: 0, getLine: () => undefined } },
      parser: { registerOscHandler: () => ({ dispose() {} }) },
      write(data, callback) {
        if (typeof data === 'string') writes.push(data);
        if (callback) callbacks.push(callback);
      },
      resize() {},
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

describe('RegionWriter lines Vosh prints about itself', () => {
  const line = (text: string): RegionOutput => ({ text: `${text}\r\n`, fresh: true });

  it('starts a row of its own after a prompt that came after your echo', async () => {
    const { term, writer } = setup();
    writer.output({ text: 'room\r\n' });
    writer.local('#walk stop\r\n');
    writer.output({ text: '<1020hp 800m> ' });
    writer.output(line('[walk] You are not walking.'));
    await parsed(writer);
    expect(screen(term)).toEqual([
      'room',
      '#walk stop',
      '<1020hp 800m> ',
      '[walk] You are not walking.',
    ]);
  });

  it('adds no blank row at the start of a row or after held line ends', async () => {
    const { term, writer } = setup();
    writer.output({ text: '<1020hp 800m> ' });
    writer.local('#walk stop\r\n');
    writer.output(line('[walk] You are not walking.'));
    writer.output({ text: 'room', hold: '\r\n' });
    writer.output(line('[lua] boom'));
    await parsed(writer);
    expect(screen(term)).toEqual([
      '<1020hp 800m> #walk stop',
      '[walk] You are not walking.',
      'room',
      '[lua] boom',
    ]);
  });

  it('leaves the echo of a command Vosh draws itself after the prompt', async () => {
    const { term, writer } = setup();
    writer.output({ text: '<1020hp 800m> ' });
    writer.output({ text: 'kick goblin\r\n' });
    await parsed(writer);
    expect(screen(term)).toEqual(['<1020hp 800m> kick goblin']);
  });
});

describe('RegionWriter held line ends', () => {
  const cursor = (term: Terminal) => [term.buffer.active.cursorY, term.buffer.active.cursorX];

  it('keeps them back until the next session text lands, and writes them once', async () => {
    const { term, writer } = setup();
    writer.output({ text: 'room\r\n[Exits: south]', hold: '\r\n\r\n' });
    await parsed(writer);
    expect(screen(term)).toEqual(['room', '[Exits: south]']);
    expect(cursor(term)).toEqual([1, 14]);
    expect(writer.pendingRows()).toBe(2);
    writer.output({ text: 'tell', hold: '\r\n\r\n' });
    await parsed(writer);
    expect(screen(term)).toEqual(['room', '[Exits: south]', '', 'tell']);
  });

  it('leaves the cells a replace erases on the default background', async () => {
    // The region's line ends on a background, and its color reset waits
    // with its line end, so the background is still on when the replace
    // erases.
    const { term, writer } = setup(20, 6);
    writer.output({ text: `${mark(1)}\x1b[44mhungry`, hold: '\x1b[0m\r\n' });
    writer.output(replace(1, `${mark(2)}\x1b[44mHUNGRY`));
    await parsed(writer);
    expect(screen(term)).toEqual(['HUNGRY']);
    const buffer = term.buffer.active;
    const cell = (y: number, x: number) => buffer.getLine(buffer.baseY + y)?.getCell(x);
    expect(cell(0, 0)?.getBgColor()).toBe(4);
    for (let y = 0; y < 6; y++) {
      for (let x = y === 0 ? 6 : 0; x < 20; x++) {
        expect(cell(y, x)?.isBgDefault(), `row ${y}, column ${x}`).toBe(true);
      }
    }
  });

  it('writes them before your echo, whichever reaches xterm first', async () => {
    for (const echoFirst of [true, false]) {
      const { term, writer } = setup();
      writer.output({ text: 'room', hold: '\r\n\r\n' });
      const reply = { text: 'The Bank of Aabahran', hold: '\r\n\r\n' };
      if (echoFirst) {
        writer.local('look\r\n');
        writer.output(reply);
      } else {
        writer.output(reply);
        writer.local('look\r\n');
      }
      await parsed(writer);
      expect(screen(term)).toEqual(
        echoFirst
          ? ['room', '', 'look', 'The Bank of Aabahran']
          : ['room', '', 'The Bank of Aabahran', '', 'look'],
      );
    }
  });

  it('leaves the text on your echo of a blank line as on the last line of a reply', async () => {
    // The game answers a blank line with a line end and the prompt, and
    // a command with its lines, a line end and the prompt.
    const after = async (line: string, reply: string) => {
      const { term, writer } = setup();
      writer.output({ text: 'room', hold: '\r\n\r\n', pinRow: true });
      writer.local(`${ECHO_CARET}${line}\r\n`);
      writer.output({ text: reply, hold: '\r\n\r\n'.slice(reply ? 0 : 2), pinRow: true });
      await parsed(writer);
      const rows = screen(term);
      const waiting = [rows, cursor(term)[0] - (rows.length - 1)] as const;
      writer.output({ text: 'tell' });
      await parsed(writer);
      return [...waiting, screen(term)] as const;
    };
    const [look, lookBelow, lookThen] = await after('look', 'The Bank of Aabahran');
    const [blank, blankBelow, blankThen] = await after('', '');
    expect(look).toEqual(['room', '', '\u203a look', 'The Bank of Aabahran']);
    expect(blank).toEqual(['room', '', '\u203a ']);
    expect(lookBelow).toBe(0);
    expect(blankBelow).toBe(0);
    expect(lookThen.slice(look.length)).toEqual(['', 'tell']);
    expect(blankThen.slice(blank.length)).toEqual(['', 'tell']);
  });

  it('keeps the longer hold through an output that writes nothing', async () => {
    const { term, writer } = setup();
    writer.output({ text: 'room', hold: '\r\n\r\n' });
    writer.output({ text: '', hold: '\r\n' });
    writer.output({ text: '' });
    expect(writer.pendingRows()).toBe(2);
    writer.output({ text: 'tell' });
    await parsed(writer);
    expect(screen(term)).toEqual(['room', '', 'tell']);
    expect(writer.pendingRows()).toBe(0);
  });

  it('replaces the open region before them and writes a fresh replace after them', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}partial` });
    writer.output({ text: '', hold: '\r\n' });
    writer.output(replace(1, `${mark(2)}WHOLE`));
    await parsed(writer);
    expect(screen(term)).toEqual(['WHOLE']);
    writer.output({ text: 'and more' });
    writer.output({ text: '', hold: '\r\n\r\n' });
    writer.output(replace(9, 'fresh\r\n', true));
    await parsed(writer);
    expect(screen(term)).toEqual(['WHOLE', 'and more', '', 'fresh']);
  });
});

describe('the row a pinned prompt leaves open', () => {
  // A pinned prompt left the text, but the row it held is still where
  // the next thing lands. Whatever would have ended that row first, a
  // framed echo from outside a read or an error notice the page writes,
  // ends a row that is not there, so that line end writes nothing.
  it('finds the line end that ends it past escapes, and only before text', () => {
    expect(closePinRow('\r\nTICK\r\n')).toEqual({ text: 'TICK\r\n', closed: true });
    expect(closePinRow('\x1b[33m\r\nTICK')).toEqual({ text: '\x1b[33mTICK', closed: true });
    expect(closePinRow('look\r\n')).toEqual({ text: 'look\r\n', closed: true });
    expect(closePinRow('\n')).toEqual({ text: '', closed: true });
    expect(closePinRow('\x1b[0m')).toEqual({ text: '\x1b[0m', closed: false });
    expect(closePinRow('')).toEqual({ text: '', closed: false });
  });

  it('lets a framed echo and a notice take the prompt row', async () => {
    const { term, writer } = setup();
    writer.output({ text: 'room\r\n[Exits: south]', hold: '\r\n\r\n', pinRow: true });
    // The tick warning, from outside a read.
    writer.output({ text: '\r\n\x1b[33mTICK IN 5s\x1b[0m\r\n' });
    // The next pulse, which the session starts with a line end since the
    // warning closed the row.
    writer.output({ text: '\r\ntell\r\n', hold: '\r\n', pinRow: true });
    writer.local('\r\n\x1b[31m[Not connected]\x1b[0m\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual([
      'room',
      '[Exits: south]',
      '',
      'TICK IN 5s',
      '',
      'tell',
      '',
      '[Not connected]',
    ]);
  });

  it('keeps every line end once the row closed, or with no row open', async () => {
    const { term, writer } = setup();
    writer.output({ text: 'room', hold: '\r\n\r\n', pinRow: true });
    writer.local('look\r\n');
    writer.output({ text: '\r\nreply\r\n' });
    // A prompt that took its line end leaves no row open.
    writer.output({ text: '', hold: '\r\n', pinRow: false });
    writer.local('\r\n[notice]\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual(['room', '', 'look', '', 'reply', '', '', '[notice]']);
  });

  it('stays open through an output that writes nothing', async () => {
    const { term, writer } = setup();
    writer.output({ text: 'room', hold: '\r\n\r\n', pinRow: true });
    writer.output({ text: '', pinRow: true });
    writer.output({ text: '\x1b[0m' });
    writer.output({ text: '\r\nTICK\r\n' });
    await parsed(writer);
    expect(screen(term)).toEqual(['room', '', 'TICK']);
  });
});

describe('the lines above a region on a change of where your prompt shows', () => {
  const tank = 'Tester: [===|===|===|---]';
  const drawn = (gen: number) => `wounds.\r\n\r\n${tank}\r\n${mark(gen)}<765>`;
  const moved = (gen: number, text: string, above: string, plain = tank): RegionOutput => ({
    text: '',
    replace: { gen, text, fresh: false, above: { plain, text: above } },
  });

  it('erases the tank line with the region when it sits right above it', async () => {
    const { term, writer } = setup();
    writer.output({ text: drawn(1) });
    writer.output(moved(1, '', ''));
    await parsed(writer);
    expect(screen(term)).toEqual(['wounds.']);
  });

  it('finds the lines however narrow the terminal wrapped them', async () => {
    const { term, writer } = setup(12);
    writer.output({ text: drawn(1) });
    writer.output(moved(1, '', ''));
    await parsed(writer);
    expect(screen(term)).toEqual(['wounds.']);
  });

  it('replaces only the region when the lines above show something else', async () => {
    const { term, writer } = setup();
    writer.output({ text: drawn(1) });
    writer.output(moved(1, `${mark(2)}NEW`, 'never', 'Somebody else: [---]'));
    await parsed(writer);
    expect(screen(term)).toEqual(['wounds.', '', tank, 'NEW']);
  });

  it('tells who asks where a replace that writes nothing erased from', async () => {
    const { writer } = setup();
    const erased: [number, number][] = [];
    writer.onErase((row, col) => erased.push([row, col]));
    writer.output({ text: drawn(1) });
    writer.output(moved(1, '', ''));
    writer.output({ text: `x${mark(3)}more` });
    writer.output(replace(3, `${mark(4)}again`));
    writer.output(replace(4, ''));
    await parsed(writer);
    expect(erased).toEqual([
      [2, 0],
      [2, 1],
    ]);
  });
});

// The prompt card lays the open row out from where its region starts in
// xterm's own buffer, to map a pointer to a piece.
describe('where the open region starts', () => {
  it('names the row and column of its first cell once xterm parsed its mark', async () => {
    const { writer } = setup(40, 10);
    writer.output({ text: `You are hungry.\r\n${mark(3)}Tank 100%\r\n<1020hp> ` });
    await parsed(writer);
    expect(writer.region()).toEqual({ gen: 3, row: 1, col: 0 });
    // A region that starts mid row.
    writer.output({ text: `\r\n<10hp> ${mark(4)}more` });
    await parsed(writer);
    expect(writer.region()).toEqual({ gen: 4, row: 3, col: 7 });
  });

  it('starts it on the next row when its mark came after a full row', async () => {
    const { writer } = setup(10, 10);
    writer.local('0123456789');
    writer.output({ text: `${mark(5)}PROMPT` });
    await parsed(writer);
    expect(writer.region()).toEqual({ gen: 5, row: 1, col: 0 });
  });

  it('follows a replace and names nothing once anything lands after it', async () => {
    const { writer } = setup(40, 10);
    writer.output({ text: `room\r\n${mark(1)}<1020hp> ` });
    writer.output(replace(1, `${mark(2)}Tank\r\n<1020hp> `));
    await parsed(writer);
    expect(writer.region()).toEqual({ gen: 2, row: 1, col: 0 });
    writer.local('look');
    await parsed(writer);
    expect(writer.region()).toBeNull();
    writer.output({ text: `\r\n${mark(3)}<1020hp> ` });
    writer.output({ text: '\r\nYou are hungry.\r\n' });
    await parsed(writer);
    expect(writer.region()).toBeNull();
  });

  it('names nothing before xterm parsed the mark', () => {
    const { writer } = setup(40, 10);
    writer.output({ text: `${mark(1)}<1020hp> ` });
    expect(writer.region()).toBeNull();
  });
});

// With the scrollback full, a narrower window wraps old lines onto more
// rows, and xterm trims the oldest to make room.
describe('the open region through a resize with a full scrollback', () => {
  /** The buffer row that starts with `text`, from the bottom. */
  const rowOf = (term: Terminal, text: string) => {
    const buffer = term.buffer.active;
    for (let y = buffer.length - 1; y >= 0; y--) {
      if (buffer.getLine(y)?.translateToString(true).startsWith(text)) return y;
    }
    return -1;
  };

  it('keeps the region on your prompt, narrower and back, and a replace still lands', async () => {
    const term = new Terminal({ cols: 80, rows: 20, scrollback: 100, allowProposedApi: true });
    const writer = new RegionWriter(term);
    const line = (n: number) => `${String(n).padStart(4, '0')} ${'x'.repeat(65)}\r\n`;
    let text = '';
    for (let n = 0; n < 300; n++) text += line(n);
    writer.output({ text: `${text}${mark(1)}<1020hp 800mn 930mv> ` });
    await parsed(writer);
    expect(writer.region()?.row).toBe(rowOf(term, '<1020hp'));

    for (const [cols, rows] of [
      [40, 14],
      [80, 20],
      [30, 12],
    ]) {
      term.resize(cols, rows);
      await parsed(writer);
      const at = rowOf(term, '<1020hp');
      expect(at, `${cols}x${rows}`).toBeGreaterThan(0);
      expect(writer.region(), `${cols}x${rows}`).toEqual({ gen: 1, row: at, col: 0 });
    }

    writer.output(replace(1, `${mark(2)}<100%> `));
    await parsed(writer);
    expect(screen(term).slice(-1)).toEqual(['<100%> ']);
    expect(screen(term).join('\n')).not.toContain('1020hp');
  });

  it('keeps a two row prompt whole, its long first row wrapped at each width', async () => {
    const term = new Terminal({ cols: 80, rows: 20, scrollback: 100, allowProposedApi: true });
    const writer = new RegionWriter(term);
    let text = '';
    for (let n = 0; n < 300; n++) text += `${String(n).padStart(4, '0')} ${'x'.repeat(65)}\r\n`;
    const tank = `Tamwell: ${'='.repeat(50)}`;
    writer.output({ text: `${text}You hit.${mark(1)}${tank}\r\n<1020hp 800mn> ` });
    await parsed(writer);
    expect(writer.region()).toEqual({ gen: 1, row: rowOf(term, 'You hit.Tamwell'), col: 8 });

    for (const [cols, rows] of [
      [40, 14],
      [80, 20],
      [24, 12],
    ]) {
      term.resize(cols, rows);
      await parsed(writer);
      const at = rowOf(term, 'You hit.');
      expect(at, `${cols}x${rows}`).toBeGreaterThan(0);
      expect(writer.region(), `${cols}x${rows}`).toEqual({ gen: 1, row: at, col: 8 });
    }

    writer.output(replace(1, `${mark(2)}<100%> `));
    await parsed(writer);
    expect(screen(term).slice(-1)).toEqual(['You hit.<100%> ']);
    expect(screen(term).join('\n')).not.toContain('Tamwell');
  });
});

// A resize pads the screen so the cursor sits on its last row. A design
// that lost a row leaves your prompt above that row, and padding written
// after the open region would close it while the session still repaints
// it, with the cursor on a row of its own below your prompt.
describe('padding the screen down to its last row', () => {
  const cursor = (term: Terminal) => [term.buffer.active.cursorY, term.buffer.active.cursorX];

  it('writes the line ends while no region is open', async () => {
    const { term, writer } = setup(40, 10);
    writer.output({ text: 'You are hungry.\r\n' });
    writer.pad('\r\n'.repeat(8));
    await parsed(writer);
    expect(cursor(term)).toEqual([9, 0]);
  });

  it('writes nothing after the open region, so a later replace still lands', async () => {
    const { term, writer } = setup(40, 10);
    writer.output({ text: `You are hungry.\r\n${mark(1)}Tank 100%\r\n<1020> ` });
    writer.output(replace(1, `${mark(2)}SHORT> `));
    await parsed(writer);
    writer.pad('\r\n'.repeat(8));
    await parsed(writer);
    expect(cursor(term)).toEqual([1, 7]);
    expect(writer.region()).toEqual({ gen: 2, row: 1, col: 0 });
    writer.output(replace(2, `${mark(3)}EDITED> `));
    writer.local('look\r\n');
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', 'EDITED> look']);
  });

  it('replaces the open region after the screen grows or shrinks', async () => {
    const { term, writer } = setup(40, 10);
    writer.output({ text: `You are hungry.\r\n${mark(1)}<1020> ` });
    await parsed(writer);
    let gen = 1;
    for (const rows of [14, 6]) {
      term.resize(40, rows);
      writer.pad('\r\n'.repeat(rows));
      writer.output(replace(gen, `${mark(gen + 1)}[${rows}] `));
      gen += 1;
      await parsed(writer);
      expect(screen(term).slice(-1), `${rows} rows`).toEqual([`[${rows}] `]);
      expect(screen(term).join('\n')).not.toContain('1020');
    }
  });

  it('keeps a preview on the open region until the session clears it', async () => {
    const { term, writer } = setup(40, 10);
    writer.output({ text: `You are hungry.\r\n${mark(1)}<180> `, restore: `${mark(1)}<1020> ` });
    await parsed(writer);
    writer.pad('\r\n'.repeat(8));
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', '<180> ']);
    writer.output(replace(1, `${mark(2)}<1020> `));
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', '<1020> ']);
    expect(writer.region()).toEqual({ gen: 2, row: 1, col: 0 });
  });
});

describe('a resize while xterm parses a long backlog', () => {
  // xterm parses for about 12 ms at a time. It ends a slice once a write
  // it parsed takes the clock 12 ms past the start of the slice, and a
  // warm parser takes this backlog in less. The clock below moves 12 ms
  // each time xterm reads it, so the backlog always fills a slice of its
  // own and the line after it waits for the next one. xterm flushes what
  // it holds before a resize, and that flush starts from the first write
  // it still keeps, the parsed backlog included.
  const backlog = ['BEGIN', ...Array.from({ length: 40_000 }, (_, i) => `line ${i}`), 'END'];

  afterEach(() => {
    vi.restoreAllMocks();
  });

  /** Every row xterm holds, scrollback included, trailing blanks
   *  trimmed. */
  function allRows(term: Terminal): string[] {
    const buffer = term.buffer.active;
    const rows: string[] = [];
    for (let y = 0; y < buffer.length; y++) {
      rows.push(buffer.getLine(y)?.translateToString(true) ?? '');
    }
    while (rows.length > 0 && rows[rows.length - 1] === '') rows.pop();
    return rows;
  }

  /** Write the backlog and a line after it, and call `resize` between
   *  the slice that parses the backlog and the next. */
  async function resizeBetweenSlices(resize: (term: Terminal, writer: RegionWriter) => void) {
    let clock = 0;
    vi.spyOn(performance, 'now').mockImplementation(() => (clock += 12));
    const term = new Terminal({ cols: 40, rows: 10, scrollback: 100_000, allowProposedApi: true });
    const writer = new RegionWriter(term);
    const between = new Promise<void>((resolve) => {
      const first = term.onWriteParsed(() => {
        first.dispose();
        queueMicrotask(() => {
          resize(term, writer);
          resolve();
        });
      });
    });
    writer.local(`${backlog.join('\r\n')}\r\n`);
    writer.local('after\r\n');
    await between;
    await parsed(writer);
    return { term, rows: allRows(term) };
  }

  it('shows the backlog twice when xterm resizes on its own', async () => {
    const { rows } = await resizeBetweenSlices((term) => term.resize(40, 14));
    expect(rows.filter((row) => row === 'BEGIN')).toHaveLength(2);
  });

  it('shows the backlog once when the writer holds the resize', async () => {
    const { term, rows } = await resizeBetweenSlices((_, writer) => writer.resize(40, 14));
    expect(rows.filter((row) => row === 'BEGIN')).toHaveLength(1);
    expect(rows.slice(-2)).toEqual(['END', 'after']);
    expect([term.cols, term.rows]).toEqual([40, 14]);
  });

  it('resizes at once while xterm holds no write', async () => {
    const { term, writer } = setup(40, 10);
    writer.local('You are hungry.\r\n');
    await parsed(writer);
    writer.resize(30, 12);
    expect([term.cols, term.rows]).toEqual([30, 12]);
  });

  it('resizes at once while xterm has parsed none of what it holds', async () => {
    // xterm flushes from its first write, and parses each one once. The
    // rows the pinned band lends go in before the page paints.
    const { term, writer } = setup(40, 10);
    writer.local('prompt> \r\n');
    writer.resize(40, 9);
    expect([term.cols, term.rows]).toEqual([40, 9]);
    await parsed(writer);
    expect(screen(term)).toEqual(['prompt> ']);
  });

  it('takes the last size once xterm parsed its writes, before the writes after it', async () => {
    const sizes: number[][] = [];
    const { rows } = await resizeBetweenSlices((term, writer) => {
      writer.resize(30, 12);
      writer.resize(20, 14);
      sizes.push([term.cols, term.rows]);
      writer.whenParsed(() => sizes.push([term.cols, term.rows]));
      writer.local('look\r\n');
    });
    expect(sizes).toEqual([
      [40, 10],
      [20, 14],
    ]);
    expect(rows.slice(-3)).toEqual(['END', 'after', 'look']);
  });

  it('keeps the size xterm has when a later resize asks for it back', async () => {
    const { term } = await resizeBetweenSlices((_, writer) => {
      writer.resize(30, 12);
      writer.resize(40, 10);
    });
    expect([term.cols, term.rows]).toEqual([40, 10]);
  });

  it('stops counting a write xterm refused', () => {
    // xterm throws a write away while it holds too much it has not
    // parsed yet.
    const writes: string[] = [];
    const callbacks: (() => void)[] = [];
    const sizes: number[][] = [];
    const term: RegionTerminal = {
      cols: 40,
      buffer: { active: { cursorX: 0, cursorY: 0, baseY: 0, getLine: () => undefined } },
      parser: { registerOscHandler: () => ({ dispose() {} }) },
      write(data, callback) {
        if (data === 'refused') throw new Error('write data discarded');
        if (typeof data === 'string') writes.push(data);
        if (callback) callbacks.push(callback);
      },
      resize(cols, rows) {
        sizes.push([cols, rows]);
      },
    };
    const writer = new RegionWriter(term);
    expect(() => writer.local('refused')).toThrow();
    writer.local('You are hungry.\r\n');
    writer.local('look\r\n');
    for (const callback of callbacks.splice(0)) callback();
    writer.resize(30, 12);
    writer.local('You see nothing special.\r\n');
    expect(sizes).toEqual([[30, 12]]);
    expect(writes).toEqual(['You are hungry.\r\n', 'look\r\n', 'You see nothing special.\r\n']);
  });
});

describe('a run of repeated lines the session collapses', () => {
  // The session writes each line as a region of its own while Collapse
  // repeated lines is on, and rewrites the run's region in place with the
  // count before the line as the next line joins it. The lines are the
  // game's own, from fight.c, with an invented name.
  const dodge = "You dodge Quenby's attack.";
  const parry = "You parry Quenby's attack.";
  const count = (n: number, line: string) => `\x1b[0m\x1b[38;5;244m(${n}) \x1b[39m${line}`;

  it('shows the run once, its count rewritten in place, and the next line after it', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}${dodge}\r\n` });
    writer.output(replace(1, `${mark(2)}${count(2, dodge)}\r\n`, true));
    writer.output(replace(2, `${mark(3)}${count(3, dodge)}\r\n`, true));
    writer.output({ text: `${mark(4)}${parry}\r\n` });
    await parsed(writer);
    expect(screen(term)).toEqual([`(3) ${dodge}`, parry]);
  });

  it('writes the run on a new row once your echo landed after it', async () => {
    // Your echo reached xterm before the session heard of it.
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}${dodge}\r\n` });
    writer.local('wake\r\n');
    writer.output(replace(1, `${mark(2)}${count(2, dodge)}\r\n`, true));
    writer.output(replace(2, `${mark(3)}${count(3, dodge)}\r\n`, true));
    await parsed(writer);
    expect(screen(term)).toEqual([dodge, 'wake', `(3) ${dodge}`]);
  });

  it('goes on in place in the history of the split, which took the run from your scrollback', async () => {
    // The history pane fills from the scrollback ring, which keeps the
    // run as the screen shows it, its region marked while it is the last
    // line. Your prompt shows pinned, so the session rewrites the run
    // without the line end the live screen still holds back, and hands it
    // over as the tail, which this pane, holding nothing, holds instead.
    const { term, writer } = setup(40, 6);
    writer.local(`You are hungry.\r\n${mark(7)}${count(2, dodge)}\r\n`);
    const tail = (gen: number, text: string): RegionOutput => ({
      text: '',
      replace: { gen, text, fresh: true, tail: '\r\n' },
    });
    writer.output(tail(7, `${mark(8)}${count(3, dodge)}`));
    writer.output(tail(8, `${mark(9)}${count(4, dodge)}`));
    writer.output({ text: `${mark(10)}${parry}\r\n` });
    await parsed(writer);
    expect(screen(term)).toEqual(['You are hungry.', `(4) ${dodge}`, parry]);
  });

  it('holds the tail back once it writes the run on a new row', async () => {
    // Your echo reached xterm after the run, and before the session heard
    // of it, so the run goes on on a new row. Its line end still waits
    // there, and the next line lands on a row of its own in its own color.
    const { term, writer } = setup();
    const red = `\x1b[1;31m${dodge}`;
    writer.output({ text: `${mark(1)}${red}`, hold: '\x1b[0m\r\n', pinRow: true });
    writer.local('kill guard\r\n');
    writer.output({
      text: '',
      replace: { gen: 1, text: `${mark(2)}${count(2, red)}`, fresh: true, tail: '\x1b[0m\r\n' },
      pinRow: true,
    });
    writer.output({ text: `${mark(3)}${parry}\r\n` });
    await parsed(writer);
    expect(screen(term)).toEqual([dodge, 'kill guard', `(2) ${dodge}`, parry]);
    const buffer = term.buffer.active;
    expect(
      buffer
        .getLine(buffer.baseY + 3)
        ?.getCell(0)
        ?.isFgDefault(),
    ).toBe(true);
  });

  it('keeps what you read where it is while the run goes on below', async () => {
    const { term, writer } = setup(40, 5);
    const history = Array.from({ length: 20 }, (_, i) =>
      i % 2 === 0 ? 'You are hungry.' : 'You are thirsty.',
    );
    writer.output({ text: `${history.join('\r\n')}\r\n${mark(1)}${dodge}\r\n` });
    await parsed(writer);
    term.scrollLines(-6);
    const top = term.buffer.active.viewportY;
    const reading = term.buffer.active.getLine(top)?.translateToString(true);
    writer.output(replace(1, `${mark(2)}${count(2, dodge)}\r\n`, true));
    writer.output(replace(2, `${mark(3)}${count(3, dodge)}\r\n`, true));
    await parsed(writer);
    expect(term.buffer.active.viewportY).toBe(top);
    expect(term.buffer.active.getLine(top)?.translateToString(true)).toBe(reading);
    expect(screen(term).slice(-2)).toEqual(['You are thirsty.', `(3) ${dodge}`]);
  });

  it('finds the run right above a region a pinned prompt emptied', async () => {
    const { term, writer } = setup();
    writer.output({ text: `${mark(1)}${dodge}\r\n${mark(2)}[1020/1020hp 8` });
    // The prompt leaves the text, and its region stays open, empty.
    writer.output(replace(2, mark(3)));
    writer.output({
      text: '',
      replace: {
        gen: 3,
        text: `${mark(4)}${count(2, dodge)}\r\n`,
        fresh: true,
        above: { plain: dodge, text: `${mark(4)}${count(2, dodge)}\r\n` },
      },
    });
    writer.output(replace(4, `${mark(5)}${count(3, dodge)}\r\n`, true));
    await parsed(writer);
    expect(screen(term)).toEqual([`(3) ${dodge}`]);
  });
});

// Mark your commands draws a grey › before your echo, unless the row it
// lands on already ends in > before the cursor, as a game's own prompt
// does. The same rules as the native grid's, in
// src-tauri/src/native/grid/regions.rs. The game lines are Aabahran's own,
// from tables.c, update.c and the prompt fixtures.
describe('the mark before your echo', () => {
  const sends: [string, (writer: RegionWriter, command: string) => void][] = [
    ['typed', (writer, command) => writer.local(`${ECHO_CARET}${command}\r\n`)],
    ['quick key', (writer, command) => writer.output({ text: `${ECHO_CARET}${command}\r\n` })],
  ];
  const motd = 'Prepare yourself. For you are about to <Enter> the Forsaken Lands!';

  for (const [how, send] of sends) {
    it(`drops after a login prompt, ${how}`, async () => {
      const { term, writer } = setup();
      writer.output({ text: '\n\rAccount name> ' });
      send(writer, 'Tolliver');
      writer.output({ text: '\n\rYour choice> ' });
      send(writer, '1');
      await parsed(writer);
      expect(screen(term)).toEqual(['', 'Account name> Tolliver', '', 'Your choice> 1']);
    });

    it(`drops after your prompt in the text, ${how}`, async () => {
      const { term, writer } = setup();
      writer.output({ text: `You are hungry.\r\n${mark(1)}<1020hp 800m 930mv> ` });
      send(writer, 'look');
      await parsed(writer);
      expect(screen(term)).toEqual(['You are hungry.', '<1020hp 800m 930mv> look']);
    });

    it(`stays on an empty row and after another character, ${how}`, async () => {
      const { term, writer } = setup(80);
      writer.output({ text: 'You are hungry.\r\n' });
      send(writer, 'look');
      writer.output({ text: motd });
      send(writer, 'look');
      await parsed(writer);
      expect(screen(term)).toEqual(['You are hungry.', '› look', `${motd}› look`]);
      // A prompt that fills its row sends your echo to the next one.
      const full = setup(19);
      full.writer.output({ text: '<1020hp 800m 930mv>' });
      send(full.writer, 'look');
      await parsed(full.writer);
      expect(screen(full.term)).toEqual(['<1020hp 800m 930mv>', '› look']);
    });

    it(`stays on the row held line ends start, ${how}`, async () => {
      const { term, writer } = setup();
      writer.output({ text: '<1020hp 800m 930mv> ', hold: '\r\n' });
      send(writer, 'look');
      await parsed(writer);
      expect(screen(term)).toEqual(['<1020hp 800m 930mv> ', '› look']);
    });

    it(`stays on the row a pinned prompt left, ${how}`, async () => {
      const { term, writer } = setup();
      writer.output({ text: 'You are hungry.', hold: '\r\n\r\n', pinRow: true });
      send(writer, 'look');
      await parsed(writer);
      expect(screen(term)).toEqual(['You are hungry.', '', '› look']);
    });

    it(`drops after a lifted prompt, ${how}`, async () => {
      // The space after the band keeps your echo a cell away, on the
      // prompt's row. The lift marks go to a tracker, as in the app.
      const { term, writer } = setup();
      const lifts = new LiftTracker(term);
      writer.output({ text: `${liftStart(3)}${mark(4)}<1020hp 800m 930mv>${liftEnd(3)} ` });
      send(writer, 'look');
      await parsed(writer);
      expect(screen(term)).toEqual(['<1020hp 800m 930mv> look']);
      expect(lifts.size).toBe(1);
      lifts.dispose();
    });

    it(`reads the row once the live render is back, ${how}`, async () => {
      const drawn = '[1020/1020hp 800/800mn 930/930mv] ';
      const game = '<1020hp 800m 930mv> ';
      // The card previews your design over the game's own line.
      const first = setup();
      first.writer.output({ text: `${mark(1)}${drawn}`, restore: `${mark(1)}${game}` });
      send(first.writer, 'look');
      await parsed(first.writer);
      expect(screen(first.term)).toEqual(['<1020hp 800m 930mv> look']);
      // The card previews the game's line over your design.
      const second = setup();
      second.writer.output({ text: `${mark(1)}${game}`, restore: `${mark(1)}${drawn}` });
      send(second.writer, 'look');
      await parsed(second.writer);
      expect(screen(second.term)).toEqual([`${drawn}› look`]);
    });
  }

  it('drops before a bare line end at a login prompt', async () => {
    const { term, writer } = setup();
    writer.output({ text: '\n\rYour choice> ' });
    writer.local(`${ECHO_CARET}\r\n`);
    writer.output({ text: '\n\rYour choice> ' });
    await parsed(writer);
    expect(screen(term)).toEqual(['', 'Your choice> ', '', 'Your choice> ']);
  });
});
