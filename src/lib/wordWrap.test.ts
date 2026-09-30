import { describe, expect, it } from 'vitest';
import fixture from '../../fixtures/wrap/cases.json';
import { WordWrapper } from './wordWrap';

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
