import { act, createElement, createRef } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import type { SessionRowState } from '../stores/session/sessionRowStore';
import sessionsCss from '../styles/sessions.css?raw';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import type { SessionLine } from './sessionLine';
import { SessionSidebar, type SessionSidebarHandle } from './SessionSidebar';

// The sessions sidebar, with its two line rows. Each row reads its
// session as sessionLabel names it, the selected one marked current, a
// port that is not the world port in quiet meta, and a row with neither
// a name nor a character named by its world with the port kept apart.
// A row takes the look the row store gives it and the second line
// useSessionLine gives it, each faked here for each session.

/** What the row store says of each session, by id. A session it names
 *  nothing for reads quiet. */
const states = vi.hoisted(() => new Map<number, Partial<SessionRowState>>());

/** How many snoops run in each session, by id, faked. */
const snoops = vi.hoisted(() => new Map<number, number>());

vi.mock('../stores/session/snoopStore', () => ({
  useLiveSnoops: (session: number) => snoops.get(session) ?? 0,
}));

/** Whether you hold ⌘, faked. */
const mod = vi.hoisted(() => ({ held: false }));

vi.mock('./useModHeld', () => ({ useModHeld: () => mod.held }));

/** The second line of each session, by id. A session it names nothing
 *  for reads its faked row state with no GMCP. */
const lines = vi.hoisted(() => new Map<number, SessionLine>());

vi.mock('./sessionLine', async (actual) => {
  const line = await actual<typeof import('./sessionLine')>();
  const { getSessionRow } = await import('../stores/session/sessionRowStore');
  return {
    ...line,
    useSessionView: (session: number) => ({
      state: { ...getSessionRow(session), ...states.get(session) },
      room: null,
      combat: null,
      vitals: null,
      now: 0,
    }),
    useSessionLine: (row: SessionRow) =>
      lines.get(row.id) ??
      line.secondLine(
        row,
        { ...getSessionRow(row.id), ...states.get(row.id) },
        null,
        null,
        null,
        0,
      ),
  };
});

vi.mock('../stores/session/sessionRowStore', async (actual) => {
  const store = await actual<typeof import('../stores/session/sessionRowStore')>();
  return {
    ...store,
    useSessionRow: (id: number) => ({ ...store.getSessionRow(id), ...states.get(id) }),
  };
});

const PLAY = 'play.theforsakenlands.com';

function row(id: number, fields: Partial<SessionRow>): SessionRow {
  return {
    id,
    name: null,
    character: null,
    host: PLAY,
    port: 1848,
    tls: false,
    profile: 'Default',
    connected: true,
    since: null,
    selected: false,
    ...fields,
  };
}

function draw(rows: SessionRow[], selected: number): string {
  return renderToStaticMarkup(
    <SessionSidebar
      rows={rows}
      selected={selected}
      onSelect={() => undefined}
      onNewSession={() => undefined}
      onClose={() => undefined}
      onCaret={() => undefined}
      onRename={() => undefined}
      onEditConnection={() => undefined}
      onDisconnect={() => undefined}
      onMove={() => undefined}
    />,
  );
}

/** Each row button, in order. */
const buttons = (html: string) =>
  html.match(/<button[^>]*class="shell-sessions-row[^"]*"[^]*?<\/button>/g) ?? [];

/** The words in a piece of markup, hidden ones too, as a screen reader
 *  gathers them. */
const words = (html: string) => html.replace(/<[^>]*>/g, '');

afterEach(() => {
  states.clear();
  lines.clear();
  snoops.clear();
  mod.held = false;
  vi.unstubAllGlobals();
});

describe('the sessions sidebar', () => {
  const rows = [
    row(1, { character: 'Tolliver' }),
    row(2, { character: 'Orla', port: 1825 }),
    row(3, { port: 1825 }),
  ];

  it('holds New session over SESSIONS and its count, and no Hide sessions of its own', () => {
    const html = draw(rows, 1);
    expect(html).toContain('<aside class="shell-sessions st-controls" aria-label="Sessions">');
    expect(html).toContain('<div class="shell-sessions-top" data-tauri-drag-region="true">');
    expect(html).toMatch(/aria-label="New session" aria-keyshortcuts="(Meta|Control)\+T"/);
    expect(html).not.toContain('Hide sessions');
    expect(html).toContain(
      '<h2 class="shell-sessions-head">Sessions<span class="visually-hidden">, </span><span class="shell-sessions-total">3</span></h2>',
    );
  });

  it('reads Sessions 5 with five open, as board 01 draws it', () => {
    const five = [...rows, row(4, { name: 'Errands' }), row(5, { port: 1825, connected: false })];
    expect(draw(five, 1)).toContain('<span class="shell-sessions-total">5</span></h2>');
  });

  it('reads the heading as Sessions, 3, the comma hidden so it draws as before', () => {
    const head = draw(rows, 1).match(/<h2[^>]*>(.*?)<\/h2>/)?.[1] ?? '';
    expect(words(head)).toBe('Sessions, 3');
  });

  it('lists every session in order, the selected one current', () => {
    const html = draw(rows, 1);
    const [tolliver, orla, build] = buttons(html);
    expect(tolliver).toContain('aria-current="true"');
    // The card's words describe the row, which keeps no tooltip.
    expect(tolliver).toContain('aria-describedby="shell-sessions-card-1"');
    expect(tolliver).not.toContain('title=');
    expect(html).toContain(
      '<span id="shell-sessions-card-1" hidden="">The Forsaken Lands, profile Default. Double click the name to rename.</span>',
    );
    expect(tolliver).toContain(
      '<span class="shell-sessions-name"><span class="shell-sessions-name-text">Tolliver</span></span>',
    );
    expect(orla).not.toContain('aria-current');
    expect(orla).toContain(
      '<span class="shell-sessions-name-text">Orla</span><span class="shell-sessions-port">1825</span>',
    );
    expect(build).toContain(
      '<span class="shell-sessions-name-text">The Forsaken Lands</span><span class="shell-sessions-port">1825</span>',
    );
  });

  it('gives each row a close button beside it, out of the Tab order', () => {
    const html = draw(rows, 1);
    const slots = html.match(/<li class="shell-sessions-slot">[^]*?<\/li>/g) ?? [];
    expect(slots).toHaveLength(3);
    for (const slot of slots) {
      expect(slot).toMatch(
        /<\/button><button type="button" class="shell-sessions-close" aria-label="Close session" tabindex="-1"><svg[^]*<\/button><span id="shell-sessions-card-\d" hidden="">[^<]*<\/span><\/li>$/,
      );
    }
  });

  it('shows the close button in the place of the count under the pointer', () => {
    const rule = (selector: string) =>
      sessionsCss.match(new RegExp(`\\n${selector.replace(/\./g, '\\.')} \\{([^}]*)\\}`))?.[1];
    expect(rule('.shell-sessions-slot:hover .shell-sessions-end')).toMatch(/visibility: hidden;/);
    expect(rule('.shell-sessions-slot:hover .shell-sessions-close')).toMatch(
      /visibility: visible;/,
    );
    // On line one, its right edge where the count ends.
    expect(rule('.shell-sessions-close')).toMatch(/top: 7px;\s+right: 18px;/);
  });

  it('marks the session the selection moves to', () => {
    const [tolliver, orla] = buttons(draw(rows, 2));
    expect(tolliver).not.toContain('aria-current');
    expect(orla).toContain('aria-current="true"');
  });

  it('draws line two with the health at its right, in the danger tone while low', () => {
    lines.set(1, { who: null, text: 'Thickening Woods', health: 100, low: false });
    lines.set(2, { who: null, text: 'Fighting a Blackwatch guard', health: 18, low: true });
    lines.set(3, { who: 'Orla', text: 'The Bank of Aabahran', health: null, low: false });
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    expect(tolliver).toMatch(
      /<span class="shell-sessions-line">Thickening Woods<\/span><span class="shell-sessions-health"><span class="visually-hidden">Health <\/span>100%<\/span><\/button>$/,
    );
    expect(orla).toContain(
      '<span class="shell-sessions-line">Fighting a Blackwatch guard</span><span class="shell-sessions-health is-low"><span class="visually-hidden">Health </span>18%</span>',
    );
    // With no health the line takes the right column too, and a session
    // you named starts it with its character.
    expect(build).toMatch(
      /<span class="shell-sessions-line is-wide"><span class="shell-sessions-who">Orla<\/span> · The Bank of Aabahran<\/span><\/button>$/,
    );
  });

  it('says Health before the figure, so a row never reads a bare percent', () => {
    lines.set(1, { who: null, text: 'Thickening Woods', health: 94, low: false });
    const tolliver = buttons(draw(rows, 1))[0] ?? '';
    expect(words(tolliver)).toContain('Thickening WoodsHealth 94%');
  });

  it('counts what waits on a row behind in the pill, and brightens it for new lines', () => {
    states.set(1, { lines: true, waiting: ['preset:alert_tells'] });
    states.set(2, { lines: true, waiting: ['preset:alert_attacked', 'preset:alert_low_health'] });
    states.set(3, { lines: true, playing: true });
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    // The selected row shows neither, whatever its store says.
    expect(tolliver).toContain('class="shell-sessions-row"');
    expect(tolliver).toContain('<span class="shell-sessions-end"></span>');
    expect(orla).toContain('class="shell-sessions-row is-new"');
    expect(orla).toContain(
      '<span class="shell-sessions-end"><span class="shell-sessions-count" role="img" aria-label="2 waiting">2</span></span>',
    );
    expect(build).toContain('class="shell-sessions-row is-new"');
    expect(build).not.toContain('shell-sessions-count');
  });

  it('shows the eye and the count of live snoops before the waiting count, as Snoop board 05 draws it', () => {
    snoops.set(1, 2);
    snoops.set(2, 1);
    states.set(2, { waiting: ['preset:alert_tells'] });
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    const eye = (words: string) =>
      `<span class="shell-sessions-snoops" role="img" aria-label="${words}"><svg width="12" height="12"`;
    expect(tolliver).toContain(eye('2 snoops'));
    expect(tolliver).toContain('</svg><span>2</span></span></span>');
    expect(orla).toContain(eye('1 snoop'));
    expect(orla).toContain(
      '</svg><span>1</span></span><span class="shell-sessions-count" role="img" aria-label="1 waiting">1</span></span>',
    );
    expect(build).not.toContain('shell-sessions-snoops');
  });

  it('stops the count at 9+, and says the whole of it', () => {
    states.set(2, { waiting: Array.from({ length: 12 }, () => 'preset:alert_tells') });
    states.set(3, { waiting: Array.from({ length: 9 }, () => 'preset:alert_tells') });
    const [, orla, build] = buttons(draw(rows, 1));
    expect(orla).toContain('aria-label="12 waiting">9+</span>');
    expect(build).toContain('aria-label="9 waiting">9</span>');
  });

  it('starts every row with its status mark, the selected one too, and dims one not connected', () => {
    const marked = [
      row(1, { character: 'Tolliver' }),
      row(2, { port: 1825 }),
      row(3, {}),
      row(4, { character: 'Tolliver' }),
      row(5, { character: 'Orla', port: 1825, connected: false }),
    ];
    states.set(3, { link: 'dialing' });
    states.set(4, { link: 'failed' });
    const [live, login, dialing, failed, off] = buttons(draw(marked, 1));
    const mark = (kind: string, words: string) =>
      `"><span class="shell-sessions-mark is-${kind}" role="img" aria-label="${words}">`;
    expect(live).toContain(mark('live', 'Playing') + '<span class="dot is-success"></span>');
    expect(login).toContain(mark('hand', 'Logging in') + '<svg');
    expect(dialing).toContain(mark('spinner', 'Connecting') + '<svg');
    expect(failed).toContain(mark('triangle', 'Connect again') + '<svg');
    expect(off).toContain('class="shell-sessions-row is-off"');
    expect(off).toContain(mark('off', 'Not connected') + '<span class="dot is-off"></span>');
    expect(off).toContain('<span class="shell-sessions-port">1825</span>');
  });

  it('numbers the first nine rows while you hold ⌘, in the place of the count', () => {
    vi.stubGlobal('navigator', { userAgent: 'Macintosh' });
    const many = [
      row(1, { character: 'Tolliver' }),
      row(2, { character: 'Orla', port: 1825 }),
      ...Array.from({ length: 8 }, (_, i) => row(i + 3, { port: i % 2 ? 1825 : 1848 })),
    ];
    states.set(2, { waiting: ['preset:alert_tells'] });
    const quiet = buttons(draw(many, 1));
    expect(quiet.join('')).not.toContain('shell-sessions-key');
    expect(quiet[1]).toContain('shell-sessions-count');
    mod.held = true;
    const numbered = buttons(draw(many, 1));
    numbered.slice(0, 9).forEach((button, i) => {
      expect(button).toContain(
        `<span class="shell-sessions-end"><span class="shell-sessions-key" aria-hidden="true">⌘${i + 1}</span></span>`,
      );
    });
    // Orla's count gives way to her key, and her port stays.
    expect(numbered[1]).not.toContain('shell-sessions-count');
    expect(numbered[1]).toContain('<span class="shell-sessions-port">1825</span>');
    // The tenth row has no key.
    expect(numbered[9]).toContain('<span class="shell-sessions-end"></span>');
    // Held or not, the first nine rows name their keys for a screen
    // reader, and the tenth names none.
    for (const drawn of [quiet, numbered]) {
      drawn.slice(0, 9).forEach((button, i) => {
        expect(button).toContain(`aria-keyshortcuts="Meta+${i + 1}"`);
      });
      expect(drawn[9]).not.toContain('aria-keyshortcuts');
    }
  });

  it('names the key with Ctrl on Windows and Linux', () => {
    vi.stubGlobal('navigator', { userAgent: 'Windows NT 10.0' });
    mod.held = true;
    const [tolliver] = buttons(draw(rows, 1));
    expect(tolliver).toContain('<span class="shell-sessions-key" aria-hidden="true">Ctrl+1</span>');
    expect(tolliver).toContain('aria-keyshortcuts="Control+1"');
  });
});

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. This DOM sends no events. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

/** The one element under `root` that `match` finds. */
function only(root: FakeNode, what: string, match: (el: FakeElement) => boolean): FakeElement {
  const found = findAll(root, match);
  if (found.length !== 1) throw new Error(`found ${found.length} of ${what}`);
  return found[0];
}

const hasClass = (name: string) => (el: FakeElement) =>
  (el.getAttribute('class') ?? '').split(' ').includes(name);

describe('renaming and moving a session in its row', () => {
  const doc = new FakeDocument();
  /** The escape stack's keydown listener, which the window holds. */
  const windowListeners = new Map<string, Handler[]>();
  /** Hand `e` to every listener the window holds for `type`. */
  const fire = (type: string, e: unknown) => {
    for (const fn of windowListeners.get(type) ?? []) fn(e);
  };
  const rows = [row(1, { character: 'Tolliver' }), row(2, { character: 'Tolliver', port: 1825 })];
  let createRoot: typeof import('react-dom/client').createRoot;
  const cleanups: (() => Promise<void>)[] = [];

  beforeEach(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      innerWidth: 1280,
      innerHeight: 800,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener: (type: string, fn: Handler) =>
        void windowListeners.set(type, [...(windowListeners.get(type) ?? []), fn]),
      removeEventListener: (type: string, fn: Handler) =>
        void windowListeners.set(
          type,
          (windowListeners.get(type) ?? []).filter((had) => had !== fn),
        ),
    });
    vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // The field selects its text as it opens, and the row menu looks for
    // the caret and its first row. The fake DOM holds none of that.
    const el = FakeElement.prototype as unknown as Record<string, unknown>;
    el.select = () => undefined;
    // The list scrolls, and a dragged row asks for frames.
    el.scrollTop = 0;
    // The row menu measures itself as it opens, 232 wide by the recipe.
    el.offsetWidth = 232;
    el.offsetHeight = 0;
    vi.stubGlobal('requestAnimationFrame', () => 0);
    el.contains = function (this: FakeNode, other: FakeNode | null): boolean {
      for (let n = other; n; n = n.parentNode) if (n === this) return true;
      return false;
    };
    // Only a row the keyboard reached matches :focus-visible.
    el.matches = function (this: FakeElement, selector: string): boolean {
      return selector === ':focus-visible' && doc.activeElement === this;
    };
    el.querySelector = function (this: FakeElement): FakeElement | null {
      return findAll(this, (child) => child.getAttribute('role') === 'menuitem')[0] ?? null;
    };
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });

  afterEach(async () => {
    for (const cleanup of cleanups.splice(0)) await cleanup();
  });

  async function mount(shown = rows, selected = 1, takeFocus = false) {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    const handle = createRef<SessionSidebarHandle>();
    const calls = {
      onSelect: vi.fn(),
      onClose: vi.fn(),
      onCaret: vi.fn(),
      onRename: vi.fn(),
      onEditConnection: vi.fn(),
      onDisconnect: vi.fn(),
      onMove: vi.fn(),
    };
    await act(async () => {
      root.render(
        createElement(SessionSidebar, {
          ref: handle,
          rows: shown,
          selected,
          takeFocus,
          onNewSession: () => undefined,
          ...calls,
        }),
      );
    });
    cleanups.push(async () => {
      await act(async () => root.unmount());
      doc.body.removeChild(container);
    });
    const run = (fn: () => void) => act(async () => fn());
    const field = () => findAll(container, hasClass('shell-sessions-field'))[0] ?? null;
    return {
      container,
      calls,
      run,
      field,
      rename: (session: number) => run(() => handle.current?.rename(session)),
      type: (text: string) => run(() => on(field()!).onChange({ target: { value: text } })),
      enter: () =>
        run(() =>
          on(field()!).onKeyDown({
            key: 'Enter',
            nativeEvent: { isComposing: false },
            preventDefault() {},
          }),
        ),
      escape: () =>
        run(() =>
          fire('keydown', {
            key: 'Escape',
            isComposing: false,
            target: field(),
            preventDefault() {},
            stopPropagation() {},
          }),
        ),
      blur: () => run(() => on(field()!).onBlur()),
    };
  }

  it('brings the session to the front and turns its name into a field, its text selected', async () => {
    const m = await mount();
    expect(m.field()).toBeNull();
    await m.rename(2);
    expect(m.calls.onSelect).toHaveBeenCalledWith(2);
    const field = m.field();
    expect(field?.value).toBe('Tolliver');
    expect(field?.getAttribute('placeholder')).toBe('Tolliver');
    expect(doc.activeElement).toBe(field);
    // The mark stays beside it, and the row has no close button then.
    const slots = findAll(m.container, hasClass('shell-sessions-slot'));
    expect(findAll(slots[1], hasClass('shell-sessions-mark'))).toHaveLength(1);
    expect(findAll(slots[1], hasClass('shell-sessions-close'))).toHaveLength(0);
  });

  it('keeps what you typed on Return and hands the caret back', async () => {
    const m = await mount();
    await m.rename(2);
    await m.type('Builder');
    await m.enter();
    expect(m.calls.onRename).toHaveBeenCalledWith(2, 'Builder');
    expect(m.calls.onCaret).toHaveBeenCalledTimes(1);
    expect(m.field()).toBeNull();
  });

  it('leaves the row as it was on Escape', async () => {
    const m = await mount();
    await m.rename(2);
    await m.type('Builder');
    await m.escape();
    expect(m.calls.onRename).not.toHaveBeenCalled();
    expect(m.calls.onCaret).toHaveBeenCalledTimes(1);
    expect(m.field()).toBeNull();
  });

  it('keeps what you typed when you click elsewhere, and leaves the caret there', async () => {
    const m = await mount();
    await m.rename(2);
    await m.type('Builder');
    await m.blur();
    expect(m.calls.onRename).toHaveBeenCalledWith(2, 'Builder');
    expect(m.calls.onCaret).not.toHaveBeenCalled();
    expect(m.field()).toBeNull();
  });

  it('clears the name with an empty field, so the row reads the character again', async () => {
    const named = [rows[0], { ...rows[1], name: 'Builder' }];
    const m = await mount(named, 2);
    await m.rename(2);
    expect(m.calls.onSelect).not.toHaveBeenCalled();
    expect(m.field()?.value).toBe('Builder');
    expect(m.field()?.getAttribute('placeholder')).toBe('Tolliver');
    await m.type('');
    await m.enter();
    expect(m.calls.onRename).toHaveBeenCalledWith(2, null);
  });

  it('changes nothing when Return keeps the name the row read', async () => {
    const m = await mount();
    await m.rename(2);
    await m.enter();
    expect(m.calls.onRename).not.toHaveBeenCalled();
    expect(m.field()).toBeNull();
  });

  it('opens the field from a double click on the name', async () => {
    const m = await mount();
    const names = findAll(m.container, hasClass('shell-sessions-name'));
    await m.run(() => on(names[1]).onDoubleClick());
    expect(m.calls.onSelect).toHaveBeenCalledWith(2);
    expect(m.field()?.value).toBe('Tolliver');
  });

  /** Press `key` on the second row, and say whether it stopped there. */
  const press = async (m: Awaited<ReturnType<typeof mount>>, key: string) => {
    const second = findAll(m.container, hasClass('shell-sessions-row'))[1];
    let stopped = false;
    await m.run(() =>
      on(second).onKeyDown({
        key,
        nativeEvent: { isComposing: false },
        preventDefault() {},
        stopPropagation: () => (stopped = true),
      }),
    );
    return stopped;
  };

  it('opens the field from Return or F2 on a row with the keyboard, as board 04 says', async () => {
    for (const key of ['Enter', 'F2']) {
      const m = await mount();
      expect(await press(m, key)).toBe(true);
      expect(m.calls.onSelect).toHaveBeenCalledWith(2);
      expect(m.field()?.value).toBe('Tolliver');
      await m.escape();
      for (const cleanup of cleanups.splice(0)) await cleanup();
    }
  });

  it('leaves Space and other keys to the row', async () => {
    const m = await mount();
    expect(await press(m, ' ')).toBe(false);
    expect(await press(m, 'a')).toBe(false);
    expect(m.field()).toBeNull();
  });

  describe('Up and Down', () => {
    const three = [...rows, row(3, { character: 'Maren' })];
    const rowsOf = (m: Awaited<ReturnType<typeof mount>>) =>
      findAll(m.container, hasClass('shell-sessions-row'));
    /** Press `key` on whichever row has the keyboard, and say whether
     *  the row kept the key from the page. */
    const key = async (
      m: Awaited<ReturnType<typeof mount>>,
      key: string,
      mods: Partial<Record<'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey', boolean>> = {},
    ) => {
      let prevented = false;
      await m.run(() =>
        on(doc.activeElement!).onKeyDown({
          key,
          ...mods,
          nativeEvent: { isComposing: false },
          preventDefault: () => (prevented = true),
          stopPropagation() {},
        }),
      );
      return prevented;
    };

    it('move the keyboard between rows and select nothing, as board 4 says', async () => {
      const m = await mount(three);
      await m.run(() => rowsOf(m)[0].focus());
      expect(await key(m, 'ArrowDown')).toBe(true);
      expect(doc.activeElement).toBe(rowsOf(m)[1]);
      await key(m, 'ArrowDown');
      expect(doc.activeElement).toBe(rowsOf(m)[2]);
      await key(m, 'ArrowUp');
      expect(doc.activeElement).toBe(rowsOf(m)[1]);
      expect(m.calls.onSelect).not.toHaveBeenCalled();
      expect(m.calls.onCaret).not.toHaveBeenCalled();
    });

    it('stop at either end', async () => {
      const m = await mount(three);
      await m.run(() => rowsOf(m)[0].focus());
      await key(m, 'ArrowUp');
      expect(doc.activeElement).toBe(rowsOf(m)[0]);
      await m.run(() => rowsOf(m)[2].focus());
      await key(m, 'ArrowDown');
      expect(doc.activeElement).toBe(rowsOf(m)[2]);
    });

    it('leave Return and F2 to rename the row they reached', async () => {
      for (const finish of ['Enter', 'F2']) {
        const m = await mount(three);
        await m.run(() => rowsOf(m)[0].focus());
        await key(m, 'ArrowDown');
        await key(m, 'ArrowDown');
        await key(m, finish);
        expect(m.calls.onSelect).toHaveBeenCalledWith(3);
        expect(m.field()?.value).toBe('Maren');
        await m.escape();
        for (const cleanup of cleanups.splice(0)) await cleanup();
      }
    });

    it('stay put when the next row is being renamed', async () => {
      const m = await mount(three);
      await m.rename(2);
      await m.run(() => rowsOf(m)[0].focus());
      await key(m, 'ArrowDown');
      expect(doc.activeElement).toBe(rowsOf(m)[0]);
    });

    it('leave modified arrows alone', async () => {
      for (const mod of ['altKey', 'ctrlKey', 'metaKey', 'shiftKey'] as const) {
        const m = await mount(three);
        await m.run(() => rowsOf(m)[0].focus());
        expect(await key(m, 'ArrowDown', { [mod]: true })).toBe(false);
        expect(doc.activeElement).toBe(rowsOf(m)[0]);
        for (const cleanup of cleanups.splice(0)) await cleanup();
      }
    });
  });

  it('says how to finish on line two while the field is open, and keeps the mark', async () => {
    const m = await mount();
    await m.rename(2);
    const slot = findAll(m.container, hasClass('shell-sessions-slot'))[1];
    const hint = only(slot, 'the hint', hasClass('is-hint'));
    expect(hint.getAttribute('class')).toBe('shell-sessions-line is-hint');
    expect(hint.textContent).toBe('Return saves, Esc cancels');
    expect(findAll(slot, hasClass('shell-sessions-mark'))).toHaveLength(1);
    expect(findAll(slot, hasClass('shell-sessions-count'))).toHaveLength(0);
  });

  it('shows F2 beside Rename session… only when the row had the keyboard', async () => {
    const m = await mount();
    const second = findAll(m.container, hasClass('shell-sessions-row'))[1];
    const open = () =>
      m.run(() =>
        on(second).onContextMenu({
          clientX: 146,
          clientY: 120,
          preventDefault() {},
          currentTarget: second,
        }),
      );
    const rename = () =>
      only(
        doc.body,
        'Rename session…',
        (el) =>
          el.getAttribute('role') === 'menuitem' &&
          (el.textContent ?? '').startsWith('Rename session…'),
      );
    await open();
    expect(rename().textContent).toBe('Rename session…');
    await m.escape();
    await m.run(() => second.focus());
    await open();
    expect(rename().textContent).toBe('Rename session…F2');
    expect(findAll(rename(), hasClass('menu-keys'))[0]?.textContent).toBe('F2');
  });

  it('opens the row menu at the pointer on a right click, as board 9 draws it', async () => {
    const m = await mount();
    const buttons = findAll(m.container, hasClass('shell-sessions-row'));
    let prevented = false;
    await m.run(() =>
      on(buttons[1]).onContextMenu({
        clientX: 146,
        clientY: 120,
        preventDefault: () => (prevented = true),
        currentTarget: buttons[1],
      }),
    );
    expect(prevented).toBe(true);
    const menu = only(doc.body, 'the row menu', (el) => el.getAttribute('role') === 'menu');
    expect(menu.getAttribute('aria-label')).toBe('Session options');
    expect([menu.style.left, menu.style.top]).toEqual(['146px', '120px']);
    const items = findAll(menu, (el) => el.getAttribute('role') === 'menuitem');
    expect(items.map((el) => el.textContent)).toEqual([
      'Rename session…',
      'Edit connection…',
      'Disconnect',
      'Close session',
    ]);
    expect(findAll(menu, hasClass('menu-sep'))).toHaveLength(1);

    // Rename session… closes the menu, brings the row to the front and
    // turns its name into a field.
    await m.run(() => on(items[0]).onClick());
    expect(findAll(doc.body, (el) => el.getAttribute('role') === 'menu')).toHaveLength(0);
    expect(m.calls.onSelect).toHaveBeenCalledWith(2);
    expect(m.field()?.value).toBe('Tolliver');
  });

  it('runs each row of the menu on the session it opened on', async () => {
    const m = await mount();
    const open = () =>
      m.run(() =>
        on(findAll(m.container, hasClass('shell-sessions-row'))[1]).onContextMenu({
          clientX: 146,
          clientY: 120,
          preventDefault() {},
          currentTarget: findAll(m.container, hasClass('shell-sessions-row'))[1],
        }),
      );
    const item = (label: string) =>
      only(
        doc.body,
        label,
        (el) => el.getAttribute('role') === 'menuitem' && el.textContent === label,
      );
    await open();
    await m.run(() => on(item('Edit connection…')).onClick());
    expect(m.calls.onSelect).toHaveBeenCalledWith(2);
    expect(m.calls.onEditConnection).toHaveBeenCalledTimes(1);
    await open();
    await m.run(() => on(item('Disconnect')).onClick());
    expect(m.calls.onDisconnect).toHaveBeenCalledWith(2);
    await open();
    await m.run(() => on(item('Close session')).onClick());
    expect(m.calls.onClose).toHaveBeenCalledWith(2);
  });

  it('offers Disconnect only while the session is connected', async () => {
    const m = await mount([rows[0], { ...rows[1], connected: false }]);
    await m.run(() =>
      on(findAll(m.container, hasClass('shell-sessions-row'))[1]).onContextMenu({
        clientX: 146,
        clientY: 120,
        preventDefault() {},
        currentTarget: findAll(m.container, hasClass('shell-sessions-row'))[1],
      }),
    );
    const items = findAll(doc.body, (el) => el.getAttribute('role') === 'menuitem');
    expect(items.map((el) => el.textContent)).toEqual([
      'Rename session…',
      'Edit connection…',
      'Close session',
    ]);
  });

  it('draws a hairline under SESSIONS once a row has passed under it', async () => {
    const m = await mount();
    const head = only(m.container, 'the header', hasClass('shell-sessions-head'));
    const list = only(m.container, 'the list', hasClass('shell-sessions-list'));
    expect(head.getAttribute('class')).toBe('shell-sessions-head');
    await m.run(() => on(list).onScroll({ currentTarget: { scrollTop: 46 } }));
    expect(head.getAttribute('class')).toBe('shell-sessions-head is-scrolled');
    await m.run(() => on(list).onScroll({ currentTarget: { scrollTop: 0 } }));
    expect(head.getAttribute('class')).toBe('shell-sessions-head');
  });

  it('lifts a row you drag, parts the others, and moves it where you let go', async () => {
    const m = await mount([rows[0], { ...rows[1], character: 'Orla' }, row(3, { port: 1825 })]);
    const pointer = (type: string, clientY?: number) => m.run(() => fire(type, { clientY }));
    const third = findAll(m.container, hasClass('shell-sessions-row'))[2];
    await m.run(() => on(third).onPointerDown({ button: 0, clientY: 154 }));
    // A press that moves less than 4 px lifts nothing.
    await pointer('pointermove', 151);
    expect(findAll(m.container, hasClass('is-lifted'))).toHaveLength(0);

    // 31 up, as frame b8-drag draws it: Orla parts to the third place,
    // and the line marks the second.
    await pointer('pointermove', 123);
    const list = only(m.container, 'the list', hasClass('shell-sessions-list'));
    expect(list.getAttribute('class')).toBe('shell-sessions-list is-dragging');
    const slots = findAll(m.container, hasClass('shell-sessions-slot'));
    expect(slots[2].getAttribute('class')).toBe('shell-sessions-slot is-lifted');
    expect(slots[2].style.transform).toBe('translateY(-31px)');
    expect(slots[1].style.transform).toBe('translateY(46px)');
    expect(slots[0].style.transform).toBeUndefined();
    const line = only(m.container, 'the line', hasClass('shell-sessions-drop'));
    expect(line.style.top).toBe('46px');

    // Letting go moves the session, and the click it ends in, which comes
    // in the same task, selects nothing.
    await m.run(() => {
      fire('pointerup', {});
      on(third).onClick({ currentTarget: third });
    });
    expect(m.calls.onMove).toHaveBeenCalledWith(3, 1);
    expect(m.calls.onSelect).not.toHaveBeenCalled();
    expect(findAll(m.container, hasClass('shell-sessions-drop'))).toHaveLength(0);
    expect(findAll(m.container, hasClass('is-lifted'))).toHaveLength(0);
  });

  it('has New session alone at its top, since the toggle hides it', async () => {
    const m = await mount();
    const top = only(m.container, 'the top', hasClass('shell-sessions-actions'));
    const labels = findAll(top, (el) => el.nodeName === 'BUTTON').map((b) =>
      b.getAttribute('aria-label'),
    );
    expect(labels).toEqual(['New session']);
  });

  it('takes the keyboard on the selected row as it slides in over the terminal', async () => {
    doc.activeElement = null;
    const m = await mount(rows, 2, true);
    const second = findAll(m.container, hasClass('shell-sessions-row'))[1];
    expect(doc.activeElement).toBe(second);
  });

  it('leaves the keyboard where it was in its column', async () => {
    doc.activeElement = null;
    await mount(rows, 2);
    expect(doc.activeElement).toBeNull();
  });

  it('keeps a press that never moves a click, and a row let go in its place where it was', async () => {
    const m = await mount();
    const second = findAll(m.container, hasClass('shell-sessions-row'))[1];
    await m.run(() => on(second).onPointerDown({ button: 0, clientY: 116 }));
    await m.run(() => fire('pointerup', {}));
    await m.run(() => on(second).onClick({ currentTarget: second }));
    expect(m.calls.onSelect).toHaveBeenCalledWith(2);

    await m.run(() => on(second).onPointerDown({ button: 0, clientY: 116 }));
    await m.run(() => fire('pointermove', { clientY: 126 }));
    await m.run(() => fire('pointerup', {}));
    expect(m.calls.onMove).not.toHaveBeenCalled();
  });
});
