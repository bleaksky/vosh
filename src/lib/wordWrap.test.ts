import { describe, expect, it } from 'vitest';
import fixture from '../../fixtures/wrap/cases.json';
import { WordWrapper, wrapBreaks } from './wordWrap';

// The Rust wrap in crates/prompt/src/wrap.rs runs the same cases, so xterm
// and the native grid break a line at the same place.
describe('WordWrapper against the shared wrap fixture', () => {
  for (const c of fixture.cases) {
    it(c.name, () => {
      const wrapper = new WordWrapper(c.cols);
      expect(wrapper.process(c.input) + wrapper.flush()).toBe(c.expected);
    });
  }
});

describe('WordWrapper across chunks', () => {
  it('holds a partial line until its end arrives', () => {
    const wrapper = new WordWrapper(10);
    expect(wrapper.process('the quick ')).toBe('');
    expect(wrapper.process('brown fox\r\n')).toBe('the quick\r\nbrown fox\r\n');
  });

  it('flushes a held partial line wrapped', () => {
    const wrapper = new WordWrapper(5);
    expect(wrapper.process('hello world')).toBe('');
    expect(wrapper.flush()).toBe('hello\r\nworld');
    expect(wrapper.flush()).toBe('');
  });
});

// Where a line breaks, by index into the line, so a layout can place each
// character the way the wrap placed it. WordWrapper writes its output
// from the same breaks.
describe('wrapBreaks', () => {
  /** `line` with each break written in, as WordWrapper writes it. */
  const apply = (line: string, cols: number): string => {
    const breaks = wrapBreaks(line, cols);
    let out = '';
    let next = 0;
    for (let i = 0; i < line.length; i++) {
      const at = breaks[next];
      if (at && at.at === i) {
        next++;
        out += '\r\n';
        if (at.replaced) continue;
      }
      out += line[i];
    }
    return out;
  };

  for (const c of fixture.cases) {
    it(`gives the breaks of the fixture: ${c.name}`, () => {
      const lines = c.input.split(/(\r|\n)/);
      const out = lines.map((part) =>
        part === '\r' || part === '\n' ? part : apply(part, c.cols),
      );
      expect(out.join('')).toBe(c.expected);
    });
  }

  it('names the space a break takes the place of, and a break inside a word', () => {
    expect(wrapBreaks('the quick brown fox', 10)).toEqual([{ at: 9, replaced: true }]);
    expect(wrapBreaks('abcdefgh', 5)).toEqual([{ at: 5, replaced: false }]);
    expect(wrapBreaks('\x1b[32m[1020/1020hp]\x1b[0m 800', 13)).toEqual([
      { at: 22, replaced: true },
    ]);
    expect(wrapBreaks('short', 40)).toEqual([]);
  });
});
