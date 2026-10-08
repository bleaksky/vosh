import { describe, expect, it } from 'vitest';
import { underlayShows, XtermMirror } from './xtermMirror';

// A pane whose screen the test hands to the native grid and back, and
// whose fills from the scrollback the test finishes when it wants.
function pane(owned: boolean) {
  const state = { owned, fills: [] as (() => void)[] };
  const written: string[] = [];
  const mirror = new XtermMirror({
    owned: () => state.owned,
    rebuild: (done) => {
      written.push('fill');
      state.fills.push(done);
    },
  });
  const write = (text: string) => mirror.write(() => written.push(text));
  return { state, written, mirror, write };
}

describe('the xterm copy under the native underlay', () => {
  it('takes every write while xterm draws the screen', () => {
    const { written, mirror, write } = pane(false);
    write('a');
    write('b');
    expect(written).toEqual(['a', 'b']);
    expect(mirror.mirrors()).toBe(true);
  });

  it('takes no write while the native grid owns the screen', () => {
    const { written, mirror, write } = pane(true);
    write('a');
    expect(written).toEqual([]);
    expect(mirror.mirrors()).toBe(false);
  });

  it('stops taking writes once the underlay comes up after the page loads', () => {
    const { state, written, write } = pane(false);
    write('scrollback');
    state.owned = true;
    write('a');
    expect(written).toEqual(['scrollback']);
  });

  it('fills anew when the screen comes back, then takes the writes that waited, in order', () => {
    const { state, written, mirror, write } = pane(true);
    write('lost');
    state.owned = false;
    mirror.check();
    write('a');
    write('b');
    expect(written).toEqual(['fill']);
    state.fills[0]();
    expect(written).toEqual(['fill', 'a', 'b']);
    write('c');
    expect(written).toEqual(['fill', 'a', 'b', 'c']);
  });

  it('fills once however often the screen is checked', () => {
    const { state, written, mirror } = pane(true);
    state.owned = false;
    mirror.check();
    mirror.check();
    expect(mirror.mirrors()).toBe(true);
    expect(written).toEqual(['fill']);
  });

  it('drops a fill the screen overtook, and its writes', () => {
    const { state, written, mirror, write } = pane(true);
    state.owned = false;
    mirror.check();
    write('a');
    // The native grid takes the screen back, then gives it up again.
    state.owned = true;
    mirror.check();
    state.owned = false;
    mirror.check();
    write('b');
    // The first fill ends late and settles nothing.
    state.fills[0]();
    expect(written).toEqual(['fill', 'fill']);
    state.fills[1]();
    expect(written).toEqual(['fill', 'fill', 'b']);
  });

  it('fills anew for a refill while xterm draws, then takes the writes that waited', () => {
    const { written, mirror, write } = pane(false);
    let finish = () => {};
    mirror.refill((done) => {
      written.push('refill');
      finish = done;
    });
    write('a');
    expect(written).toEqual(['refill']);
    finish();
    expect(written).toEqual(['refill', 'a']);
  });

  it('leaves a refill to the fill the screen coming back makes', () => {
    const { written, mirror } = pane(true);
    mirror.refill(() => written.push('refill'));
    expect(written).toEqual([]);
  });

  it('reads the underlay from the mark the root carries once the surface is up', () => {
    expect(underlayShows({ dataset: { underlay: '1' } })).toBe(true);
    expect(underlayShows({ dataset: {} })).toBe(false);
    expect(underlayShows(undefined)).toBe(false);
  });
});
