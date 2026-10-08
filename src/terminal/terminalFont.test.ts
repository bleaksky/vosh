import { describe, expect, it } from 'vitest';
import { loadDrawnFace, remeasureCell, remeasureWhenLoaded } from './terminalFont';

/** A FontFaceSet as far as the remeasure loads through one. A minted face
 *  of a family on this machine loads when `finish` runs, one for a family
 *  the machine lacks fails, and a family with no minted face resolves at
 *  once with nothing to load. */
function fakeFonts(installed: string[]) {
  const minted = new Set<string>();
  const loaded = new Set<string>();
  const asked: string[] = [];
  const waiting: (() => void)[] = [];
  return {
    minted,
    loaded,
    asked,
    finish: () => waiting.splice(0).forEach((done) => done()),
    set: {
      load(font: string): Promise<unknown> {
        asked.push(font);
        const name = font.replace(/^\d+px /, '').replace(/^"|"$/g, '');
        if (!minted.has(name)) return Promise.resolve([]);
        if (!installed.includes(name)) return Promise.reject(new Error('NetworkError'));
        return new Promise((resolve) =>
          waiting.push(() => {
            loaded.add(name);
            resolve([name]);
          }),
        );
      },
    },
  };
}

/** An xterm as far as its cell goes. Like xterm, it measures when it opens
 *  and when its font option changes to a new value, and a face still
 *  loading measures as the next family in the list that has loaded. */
function fakeTerm(list: string, loaded: Set<string>, rows: Record<string, number>) {
  let family = list;
  const measure = () => {
    const names = family.split(',').map((n) => n.trim().replace(/^"|"$/g, ''));
    const face = names.find((n) => loaded.has(n));
    return face ? rows[face] : 0;
  };
  const term = {
    row: 0,
    options: {
      fontSize: 14,
      get fontFamily() {
        return family;
      },
      set fontFamily(value: string) {
        if (value === family) return;
        family = value;
        term.row = measure();
      },
    },
  };
  term.row = measure();
  return term;
}

const BUNDLED = 'JetBrainsMono Bundled';
const INSTALLED = 'Iosevka Term';
const LIST = `"${INSTALLED}", "Fira Code", "${BUNDLED}", Menlo, monospace`;
const ROWS = { [INSTALLED]: 19, [BUNDLED]: 21.5 };

/** Let the pending promise callbacks run. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('the terminal cell after a face loads', () => {
  it('measures the face that loads after the terminal took the list', async () => {
    const fonts = fakeFonts([INSTALLED]);
    fonts.loaded.add(BUNDLED);
    const term = fakeTerm(LIST, fonts.loaded, ROWS);
    let fits = 0;
    remeasureWhenLoaded(fonts.set, term, () => fits++);
    // The page mints the faces after the terminal took the list, in the
    // same commit.
    fonts.minted.add(INSTALLED);
    expect(term.row).toBe(21.5);

    await settle();
    expect(fonts.asked).toEqual([`14px "${INSTALLED}"`]);
    expect(term.row).toBe(21.5);
    expect(fits).toBe(0);

    fonts.finish();
    await settle();
    expect(term.row).toBe(19);
    expect(term.options.fontFamily).toBe(LIST);
    expect(fits).toBe(1);
  });

  it('drops a remeasure a newer font or an unmount made stale', async () => {
    const fonts = fakeFonts([INSTALLED]);
    fonts.loaded.add(BUNDLED);
    fonts.minted.add(INSTALLED);
    const term = fakeTerm(LIST, fonts.loaded, ROWS);
    let fits = 0;
    const cancel = remeasureWhenLoaded(fonts.set, term, () => fits++);
    await settle();
    cancel();
    fonts.finish();
    await settle();
    expect(term.row).toBe(21.5);
    expect(fits).toBe(0);
  });

  it('walks past a face this machine lacks to the one that draws', async () => {
    const fonts = fakeFonts([]);
    fonts.minted.add(INSTALLED);
    fonts.minted.add('Fira Code');
    await loadDrawnFace(fonts.set, 14, LIST);
    expect(fonts.asked).toEqual([`14px "${INSTALLED}"`, '14px "Fira Code"', `14px "${BUNDLED}"`]);
  });

  it('stops at a generic family', async () => {
    const fonts = fakeFonts([]);
    await loadDrawnFace(fonts.set, 13, 'monospace, Menlo');
    expect(fonts.asked).toEqual(['13px monospace']);
  });

  it('leaves the list as it was', () => {
    const term = fakeTerm(LIST, new Set(['Menlo']), { Menlo: 20 });
    remeasureCell(term);
    expect(term.options.fontFamily).toBe(LIST);
    expect(term.row).toBe(20);
  });
});
