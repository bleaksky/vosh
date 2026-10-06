import { act, createElement, createRef } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import type { SessionRowState } from '../stores/session/sessionRowStore';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { SessionSidebar, type SessionSidebarHandle } from './SessionSidebar';

// The sessions sidebar of boards 2 and 3. Each row reads its session as
// sessionLabel names it, the selected one marked current, a port that is
// not the world port in quiet meta, and a row with neither a name nor a
// character named by its world with the port kept apart. A row takes the
// look the row store gives it, faked here for each session.

/** What the row store says of each session, by id. A session it names
 *  nothing for reads quiet. */
const states = vi.hoisted(() => new Map<number, Partial<SessionRowState>>());

/** Whether you hold ⌘, faked. */
const mod = vi.hoisted(() => ({ held: false }));

vi.mock('./useModHeld', () => ({ useModHeld: () => mod.held }));

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
      onHide={() => undefined}
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

afterEach(() => {
  states.clear();
  mod.held = false;
  vi.unstubAllGlobals();
});

describe('the sessions sidebar', () => {
  const rows = [
    row(1, { character: 'Tolliver' }),
    row(2, { character: 'Orla', port: 1825 }),
    row(3, { port: 1825 }),
  ];

  it('holds New session and Hide sessions over the SESSIONS header', () => {
    const html = draw(rows, 1);
    expect(html).toContain('<aside class="shell-sessions st-controls" aria-label="Sessions">');
    expect(html).toContain('<div class="shell-sessions-top" data-tauri-drag-region="true">');
    expect(html).toContain('aria-label="New session"');
    expect(html).toContain('aria-label="Hide sessions"');
    expect(html).toContain('<h2 class="shell-sessions-head">Sessions</h2>');
  });

  it('lists every session in order, the selected one current', () => {
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    expect(tolliver).toContain('aria-current="true"');
    expect(tolliver).toContain('title="Tolliver on The Forsaken Lands"');
    expect(tolliver).toContain('<span class="shell-sessions-name">Tolliver</span>');
    expect(tolliver).not.toContain('shell-sessions-meta');
    expect(orla).not.toContain('aria-current');
    expect(orla).toContain(
      '<span class="shell-sessions-name">Orla</span><span class="shell-sessions-meta">1825</span>',
    );
    expect(build).toContain(
      '<span class="shell-sessions-name is-world"><span class="shell-sessions-world">The Forsaken Lands</span>1825</span>',
    );
  });

  it('gives each row a close button beside it, out of the Tab order', () => {
    const html = draw(rows, 1);
    const slots = html.match(/<li class="shell-sessions-slot">[^]*?<\/li>/g) ?? [];
    expect(slots).toHaveLength(3);
    for (const slot of slots) {
      expect(slot).toMatch(
        /<\/button><button type="button" class="shell-sessions-close" aria-label="Close session" tabindex="-1"><svg[^]*<\/button><\/li>$/,
      );
    }
  });

  it('marks the session the selection moves to', () => {
    const [tolliver, orla] = buttons(draw(rows, 2));
    expect(tolliver).not.toContain('aria-current');
    expect(orla).toContain('aria-current="true"');
  });

  it('marks a row behind for new lines and alerts, with the dot in the meta place', () => {
    states.set(1, { lines: true, waiting: ['preset:alert_tells'] });
    states.set(2, { lines: true, waiting: ['preset:alert_tells'] });
    states.set(3, { lines: true, playing: true });
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    // The selected row shows neither.
    expect(tolliver).toContain('class="shell-sessions-row"');
    expect(tolliver).not.toContain('shell-sessions-glyph');
    expect(orla).toContain('class="shell-sessions-row is-new"');
    expect(orla).toContain(
      '<span class="shell-sessions-name">Orla</span><span class="shell-sessions-glyph is-dot" role="img" aria-label="Something for you"><svg',
    );
    expect(orla).not.toContain('shell-sessions-meta');
    expect(build).toContain('class="shell-sessions-row is-new"');
    expect(build).not.toContain('shell-sessions-glyph');
  });

  it('shows the link of each session as a glyph, and dims one not connected', () => {
    const glyphs = [
      row(1, { character: 'Tolliver' }),
      row(2, { port: 1825 }),
      row(3, {}),
      row(4, { character: 'Tolliver' }),
      row(5, { character: 'Orla', port: 1825, connected: false }),
    ];
    states.set(3, { link: 'dialing' });
    states.set(4, { link: 'failed' });
    const [, login, dialing, failed, off] = buttons(draw(glyphs, 1));
    const glyph = (kind: string, words: string) =>
      `<span class="shell-sessions-glyph is-${kind}" role="img" aria-label="${words}"><svg`;
    expect(login).toContain('1825</span>' + glyph('hand', 'Logging in'));
    expect(dialing).toContain(glyph('spinner', 'Connecting'));
    expect(failed).toContain(glyph('triangle', 'Connect again'));
    expect(off).toContain('class="shell-sessions-row is-off"');
    expect(off).toContain('<span class="shell-sessions-meta">1825</span>');
    expect(off).not.toContain('shell-sessions-glyph');
  });

  it('numbers the first nine rows while you hold ⌘, in place of the meta and the glyph', () => {
    vi.stubGlobal('navigator', { userAgent: 'Macintosh' });
    const many = [
      row(1, { character: 'Tolliver' }),
      row(2, { character: 'Orla', port: 1825 }),
      ...Array.from({ length: 8 }, (_, i) => row(i + 3, { port: i % 2 ? 1825 : 1848 })),
    ];
    states.set(4, { link: 'failed' });
    const quiet = buttons(draw(many, 1));
    expect(quiet.join('')).not.toContain('is-key');
    mod.held = true;
    const numbered = buttons(draw(many, 1));
    numbered.slice(0, 9).forEach((button, i) => {
      expect(button).toMatch(
        new RegExp(`</span><span class="shell-sessions-meta is-key">⌘${i + 1}</span></button>$`),
      );
      expect(button).not.toContain('shell-sessions-glyph');
    });
    // Orla's port and the failed row's triangle give way to the number.
    expect(quiet[1]).toContain('<span class="shell-sessions-meta">1825</span>');
    expect(numbered[1]).not.toContain('<span class="shell-sessions-meta">1825</span>');
    expect(quiet[3]).toContain('shell-sessions-glyph is-triangle');
    // The tenth row has no key and keeps its port.
    expect(numbered[9]).not.toContain('is-key');
    expect(numbered[9]).toContain(
      '<span class="shell-sessions-world">The Forsaken Lands</span>1825',
    );
  });

  it('names the key with Ctrl on Windows and Linux', () => {
    vi.stubGlobal('navigator', { userAgent: 'Windows NT 10.0' });
    mod.held = true;
    const [tolliver] = buttons(draw(rows, 1));
    expect(tolliver).toContain('<span class="shell-sessions-meta is-key">Ctrl+1</span>');
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
  const windowListeners = new Map<string, Handler>();
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
      addEventListener: (type: string, fn: Handler) => void windowListeners.set(type, fn),
      removeEventListener() {},
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
    vi.stubGlobal('requestAnimationFrame', () => 0);
    el.contains = function (this: FakeNode, other: FakeNode | null): boolean {
      for (let n = other; n; n = n.parentNode) if (n === this) return true;
      return false;
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

  async function mount(shown = rows, selected = 1) {
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
          onNewSession: () => undefined,
          onHide: () => undefined,
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
          windowListeners.get('keydown')?.({
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
    // The meta stays beside it, and the row has no close button then.
    const slots = findAll(m.container, hasClass('shell-sessions-slot'));
    expect(slots[1].textContent).toBe('1825');
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

  it('opens the row menu at the pointer on a right click, as board 9 draws it', async () => {
    const m = await mount();
    const buttons = findAll(m.container, hasClass('shell-sessions-row'));
    let prevented = false;
    await m.run(() =>
      on(buttons[1]).onContextMenu({
        clientX: 146,
        clientY: 120,
        preventDefault: () => (prevented = true),
      }),
    );
    expect(prevented).toBe(true);
    const menu = only(doc.body, 'the row menu', (el) => el.getAttribute('role') === 'menu');
    expect(menu.getAttribute('aria-label')).toBe('Session options');
    const items = findAll(menu, (el) => el.getAttribute('role') === 'menuitem');
    expect(items.map((el) => el.textContent)).toEqual([
      'Rename session…',
      'Edit connection…',
      'Disconnect',
      'Close session',
    ]);
    expect(findAll(menu, hasClass('shell-menu-sep'))).toHaveLength(1);

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
    await m.run(() => on(list).onScroll({ currentTarget: { scrollTop: 38 } }));
    expect(head.getAttribute('class')).toBe('shell-sessions-head is-scrolled');
    await m.run(() => on(list).onScroll({ currentTarget: { scrollTop: 0 } }));
    expect(head.getAttribute('class')).toBe('shell-sessions-head');
  });

  it('lifts a row you drag, parts the others, and moves it where you let go', async () => {
    const m = await mount([rows[0], { ...rows[1], character: 'Orla' }, row(3, { port: 1825 })]);
    const pointer = (type: string, clientY?: number) =>
      m.run(() => windowListeners.get(type)?.({ clientY }));
    const third = findAll(m.container, hasClass('shell-sessions-row'))[2];
    await m.run(() => on(third).onPointerDown({ button: 0, clientY: 154 }));
    // A press that moves less than 4 px lifts nothing.
    await pointer('pointermove', 151);
    expect(findAll(m.container, hasClass('is-lifted'))).toHaveLength(0);

    // 31 up, as frame b8-drag draws it: the row sits at 46, Orla parts
    // to the third place, and the line marks the second.
    await pointer('pointermove', 123);
    const list = only(m.container, 'the list', hasClass('shell-sessions-list'));
    expect(list.getAttribute('class')).toBe('shell-sessions-list is-dragging');
    const slots = findAll(m.container, hasClass('shell-sessions-slot'));
    expect(slots[2].getAttribute('class')).toBe('shell-sessions-slot is-lifted');
    expect(slots[2].style.transform).toBe('translateY(-31px)');
    expect(slots[1].style.transform).toBe('translateY(38px)');
    expect(slots[0].style.transform).toBeUndefined();
    const line = only(m.container, 'the line', hasClass('shell-sessions-drop'));
    expect(line.style.top).toBe('38px');

    // Letting go moves the session, and the click it ends in, which comes
    // in the same task, selects nothing.
    await m.run(() => {
      windowListeners.get('pointerup')?.({});
      on(third).onClick({ currentTarget: third });
    });
    expect(m.calls.onMove).toHaveBeenCalledWith(3, 1);
    expect(m.calls.onSelect).not.toHaveBeenCalled();
    expect(findAll(m.container, hasClass('shell-sessions-drop'))).toHaveLength(0);
    expect(findAll(m.container, hasClass('is-lifted'))).toHaveLength(0);
  });

  it('keeps a press that never moves a click, and a row let go in its place where it was', async () => {
    const m = await mount();
    const second = findAll(m.container, hasClass('shell-sessions-row'))[1];
    await m.run(() => on(second).onPointerDown({ button: 0, clientY: 116 }));
    await m.run(() => windowListeners.get('pointerup')?.({}));
    await m.run(() => on(second).onClick({ currentTarget: second }));
    expect(m.calls.onSelect).toHaveBeenCalledWith(2);

    await m.run(() => on(second).onPointerDown({ button: 0, clientY: 116 }));
    await m.run(() => windowListeners.get('pointermove')?.({ clientY: 126 }));
    await m.run(() => windowListeners.get('pointerup')?.({}));
    expect(m.calls.onMove).not.toHaveBeenCalled();
  });
});
