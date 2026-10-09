import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

// A deep link to something to pick, as Show me sends Settings to Add
// affect, rings it in this window in place of the flash.

const coach = vi.hoisted(() => ({ shown: [] as { targets: unknown[]; line: string }[] }));
vi.mock('../ui/coach', () => ({
  showCoach: ({ find, line }: { find: () => unknown[]; line: string }) =>
    coach.shown.push({ targets: find(), line }),
}));

/** An element the page draws, with the attributes it carries. */
function element(attrs: Record<string, string>) {
  const classes = new Set<string>();
  return {
    parentElement: null,
    getAttribute: (name: string) => attrs[name] ?? null,
    hasAttribute: (name: string) => name in attrs,
    getBoundingClientRect: () => ({ top: 400, height: 28 }),
    classList: { add: (c: string) => classes.add(c), remove: (c: string) => classes.delete(c) },
    classes,
    offsetWidth: 0,
  };
}

function root(found: ReturnType<typeof element>) {
  return {
    querySelector: () => found,
    getBoundingClientRect: () => ({ top: 0, height: 600 }),
    scrollTop: 0,
    scrollHeight: 2000,
    clientHeight: 600,
    scrollTo: vi.fn(),
    addEventListener() {},
    removeEventListener() {},
  };
}

let reveal: typeof import('./revealAnchor').revealSettingsAnchor;

beforeAll(async () => {
  vi.stubGlobal('window', {
    matchMedia: () => ({ matches: true }),
    setTimeout: () => 0,
    clearTimeout() {},
  });
  vi.stubGlobal('getComputedStyle', () => ({ overflowY: 'visible', scrollPaddingTop: '0' }));
  vi.stubGlobal('CSS', { escape: (s: string) => s });
  ({ revealSettingsAnchor: reveal } = await import('./revealAnchor'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe('revealSettingsAnchor', () => {
  it('rings an anchor to pick with its line, and leaves it unflashed', () => {
    const add = element({
      'data-st-anchor': 'add-affect',
      'data-st-coach': 'Pick Add affect… and name a spell you keep up.',
    });
    const page = root(add);
    reveal(page as never, ['add-affect'])();
    expect(coach.shown).toEqual([
      { targets: [add], line: 'Pick Add affect… and name a spell you keep up.' },
    ]);
    expect(add.classes.size).toBe(0);
    // Centered like a row, 400 + 14 - 300.
    expect(page.scrollTo).toHaveBeenCalledWith({ top: 114, behavior: 'auto' });
  });

  it('flashes a row and rings nothing', () => {
    coach.shown.length = 0;
    const row = element({ 'data-st-anchor': 'tick', 'data-st-flash': '' });
    reveal(root(row) as never, ['tick'])();
    expect(coach.shown).toEqual([]);
    expect(row.classes.has('st-flash')).toBe(true);
  });
});
