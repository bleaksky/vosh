import { afterEach, describe, expect, it, vi } from 'vitest';
import { goToSnoop, SNOOP_REQUEST_EVENT, snoopHasCaret } from './snoopKeys';

// Cmd J goes into the snoop from anywhere else and to the next tab from
// inside one.

/** A page whose caret sits in an element with these classes, or in none. */
function caretIn(classes: string[] | null) {
  const activeElement = classes
    ? { closest: (selector: string) => (classes.includes(selector.slice(1)) ? {} : null) }
    : null;
  return { activeElement } as unknown as Document;
}

describe('the snoop keys', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('knows when a snoop terminal holds the caret', () => {
    expect(snoopHasCaret(caretIn(['snoop-term']))).toBe(true);
    expect(snoopHasCaret(caretIn(['input-row']))).toBe(false);
    expect(snoopHasCaret(caretIn(null))).toBe(false);
  });

  it('asks to go in from the command line and to step from inside a snoop', () => {
    const asked: unknown[] = [];
    vi.stubGlobal(
      'CustomEvent',
      class {
        constructor(
          readonly type: string,
          readonly init: { detail: unknown },
        ) {}
      },
    );
    vi.stubGlobal('window', {
      dispatchEvent: (event: { type: string; init: { detail: unknown } }) =>
        asked.push([event.type, event.init.detail]),
    });
    vi.stubGlobal('document', caretIn(['input-row']));
    goToSnoop();
    vi.stubGlobal('document', caretIn(['snoop-term']));
    goToSnoop();
    expect(asked).toEqual([
      [SNOOP_REQUEST_EVENT, 'enter'],
      [SNOOP_REQUEST_EVENT, 'next'],
    ]);
  });
});
