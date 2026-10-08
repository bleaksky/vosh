import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SnoopTab } from '../ipc/snoop';
import type { Snoops } from '../stores/session/snoopStore';
import indexCss from '../styles/index.css?raw';
import snoopCss from '../styles/snoop.css?raw';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { shortcutLabel } from '../lib/shortcuts';
import { SnoopSplit } from './SnoopSplit';
import { SNOOP_REQUEST_EVENT, type SnoopRequest } from './snoopKeys';
import { endedLine, tabTitle } from './snoopLine';

// The snoop split of boards 01 to 04 of the Snoop review. The snoop
// store, the saved size and the minute clock are faked, and each snoop
// terminal stands in as a plain element that names its player, so what
// shows is what the split draws. The menu draws in place, not in a
// portal, and the find bar stands in as a plain element.

const fake = vi.hoisted(() => ({
  snoops: { tabs: [], windowed: false, selected: null, unread: new Set() } as unknown as Snoops,
  size: { share: 0.4, folded: false },
  saves: [] as unknown[],
  folds: [] as boolean[],
  selected: [] as string[],
  stops: [] as unknown[],
  closes: [] as unknown[],
  windows: [] as unknown[],
  carets: [] as string[],
  copied: [] as string[],
}));

const NOW = 1_800_000_000_000;
const MIN = 60_000;

vi.mock('../stores/session/snoopStore', () => ({
  useSnoops: () => fake.snoops,
  selectSnoop: (name: string) => {
    fake.selected.push(name);
    fake.snoops = { ...fake.snoops, selected: name };
  },
  setSnoopsFolded: (on: boolean) => fake.folds.push(on),
}));
vi.mock('../stores/config/snoopSizeStore', () => ({
  useSnoopSize: () => fake.size,
  saveSnoopSize: async (size: unknown) => void fake.saves.push(size),
}));
vi.mock('../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: unknown }) =>
    createElement('menu', { role: 'menu', 'aria-label': label }, children as never),
}));
vi.mock('../terminal/FindToolbar', () => ({
  FindToolbar: () => createElement('div', { 'data-find': '' }),
}));
vi.mock('./sessionLine', async (actual) => ({
  ...(await actual<typeof import('./sessionLine')>()),
  useMinuteClock: () => NOW,
}));
vi.mock('../ipc/snoop', () => ({
  snoopStop: async (session?: number, name?: string) => void fake.stops.push([session, name]),
  snoopClose: async (session?: number, name?: string) => void fake.closes.push([session, name]),
  snoopWindowOpen: async (session?: number) => void fake.windows.push(session),
}));
vi.mock('../terminal/SnoopTerminal', async () => {
  const { useEffect } = await import('react');
  return {
    SnoopTerminal: ({
      name,
      shown,
      onReady,
    }: {
      name: string;
      shown: boolean;
      onReady?: (handle: unknown) => void;
    }) => {
      useEffect(() => {
        onReady?.({
          findNext: () => false,
          findPrevious: () => false,
          clearSearch: () => {},
          focus: () => fake.carets.push(name),
          selection: () => `${name} selected`,
        });
        return () => onReady?.(null);
        // eslint-disable-next-line react-hooks/exhaustive-deps
      }, []);
      return createElement('div', { 'data-term': name, hidden: !shown });
    },
  };
});

const live = (name: string, last: number | null = null): SnoopTab => ({
  name,
  live: true,
  ended_at: null,
  last_output_at: last,
});
const ended = (name: string, at: number): SnoopTab => ({
  name,
  live: false,
  ended_at: at,
  last_output_at: null,
});

function snoops(
  tabs: SnoopTab[],
  selected: string | null,
  unread: string[] = [],
  windowed = false,
) {
  fake.snoops = { tabs, windowed, selected, unread: new Set(unread) };
}

const props = {
  session: 1,
  fontFamily: 'Menlo',
  fontSize: 14,
  lineHeight: 1.2,
  themeTerminalColors: false,
  onCaret: () => {},
};

const draw = () => renderToStaticMarkup(createElement(SnoopSplit, props));

/** Each tab's tag, by its player. */
function tabs(html: string): Record<string, string> {
  const found: Record<string, string> = {};
  for (const m of html.matchAll(
    /<button[^>]*role="tab"[^>]*>.*?<span class="snoop-name">([^<]*)/g,
  )) {
    found[m[1]] = m[0].match(/<button[^>]*>/)?.[0] ?? '';
  }
  return found;
}

beforeEach(() => {
  fake.size = { share: 0.4, folded: false };
  fake.saves.length = 0;
  fake.folds.length = 0;
  fake.selected.length = 0;
  fake.stops.length = 0;
  fake.closes.length = 0;
  fake.windows.length = 0;
  fake.carets.length = 0;
  fake.copied.length = 0;
});

/** The Find row, with its keys as this platform writes them. */
const FIND = `Find${shortcutLabel('Mod+F')}`;

/** The more button, closed. */
const MORE =
  '<button type="button" class="shell-icon-button snoop-more" aria-label="Snoop options" aria-haspopup="menu" aria-expanded="false"><svg';

describe('the snoop split', () => {
  it('draws nothing with no snoop, or while the snoops sit in their window', () => {
    snoops([], null);
    expect(draw()).toBe('');
    snoops([live('Tolliver')], 'Tolliver', [], true);
    expect(draw()).toBe('');
  });

  it('opens at the saved share with the eye, the tab, Stop and more (board 01)', () => {
    snoops([live('Tolliver', NOW)], 'Tolliver');
    const html = draw();
    expect(html).toContain('<section class="snoop" style="height:40%" aria-label="Snoop">');
    expect(html).toMatch(
      /<div class="snoop-strip"><span class="snoop-eye" role="img" aria-label="Snoop"><svg/,
    );
    expect(tabs(html).Tolliver).toBe(
      '<button type="button" role="tab" class="snoop-tab" aria-selected="true" title="Tolliver">',
    );
    expect(html).toContain(
      '<div class="snoop-end"><div class="snoop-act"><button type="button" class="snoop-btn">Stop</button></div>' +
        MORE,
    );
    expect(html).toContain('<div class="snoop-body"><div data-term="Tolliver"></div></div>');
    expect(html).toContain(
      '<div class="snoop-handle resizable-handle" role="separator" aria-orientation="horizontal" aria-label="Snoop height"></div>',
    );

    fake.size = { share: 0.25, folded: false };
    expect(draw()).toContain('style="height:25%"');
  });

  it('marks an ended tab, says when it ended and offers Close (board 03)', () => {
    snoops([live('Tolliver', NOW - 3 * MIN), ended('Maren', NOW - 2 * MIN)], 'Maren');
    const html = draw();
    expect(tabs(html)).toEqual({
      Tolliver:
        '<button type="button" role="tab" class="snoop-tab" aria-selected="false" title="Tolliver, quiet 3 min">',
      Maren:
        '<button type="button" role="tab" class="snoop-tab is-ended" aria-selected="true" title="Maren, ended 2 min ago">',
    });
    expect(html).toContain(
      '<div class="snoop-act"><span class="snoop-meta">Ended 2 min ago</span><button type="button" class="snoop-btn">Close</button></div>',
    );
    // Every tab keeps its terminal, and the one in front shows.
    expect(html).toContain(
      '<div data-term="Tolliver" hidden=""></div><div data-term="Maren"></div>',
    );
  });

  it('brightens a tab behind with lines you have not read', () => {
    snoops([live('Tolliver'), live('Maren'), live('Orla')], 'Tolliver', ['Maren']);
    expect(tabs(draw()).Maren).toContain('class="snoop-tab is-unread"');
  });

  it('folds to the strip, every tab kept with its mark and unread dot', () => {
    fake.size = { share: 0.4, folded: true };
    snoops([live('Tolliver'), live('Maren'), live('Orla')], 'Tolliver', ['Maren']);
    const html = draw();
    expect(html).toContain(
      '<section class="snoop is-folded" style="height:33px" aria-label="Snoop">',
    );
    expect(Object.keys(tabs(html))).toEqual(['Tolliver', 'Maren', 'Orla']);
    expect(tabs(html).Maren).toContain('class="snoop-tab is-unread"');
    // No terminal shows while folded, and each keeps its lines.
    expect(html).toContain(
      '<div data-term="Tolliver" hidden=""></div><div data-term="Maren" hidden=""></div><div data-term="Orla" hidden=""></div>',
    );
  });
});

describe('what a tab says', () => {
  it('says how long a live snoop has been quiet, in the sidebar words', () => {
    expect(tabTitle(live('Orla', NOW - 3 * MIN), NOW)).toBe('Orla, quiet 3 min');
    expect(tabTitle(live('Orla', NOW - 72 * MIN), NOW)).toBe('Orla, quiet 1 h 12 min');
    expect(tabTitle(live('Orla', NOW - 20_000), NOW)).toBe('Orla');
    expect(tabTitle(live('Orla'), NOW)).toBe('Orla');
  });

  it('says when a snoop ended, as the sidebar says a session dropped', () => {
    expect(endedLine(ended('Maren', NOW - 2 * MIN), NOW)).toBe('Ended 2 min ago');
    expect(endedLine(ended('Maren', NOW - 10_000), NOW)).toBe('Ended just now');
  });
});

describe('the strip buttons', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;
  const heard = new Map<string, Set<(event: unknown) => void>>();

  beforeAll(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener(type: string, cb: (event: unknown) => void) {
        if (!heard.has(type)) heard.set(type, new Set());
        heard.get(type)?.add(cb);
      },
      removeEventListener(type: string, cb: (event: unknown) => void) {
        heard.get(type)?.delete(cb);
      },
    });
    vi.stubGlobal('navigator', {
      userAgent: 'node',
      platform: '',
      clipboard: { writeText: async (text: string) => void fake.copied.push(text) },
    });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // The menu hangs from the more button's box.
    Object.assign(FakeElement.prototype, {
      getBoundingClientRect: () => ({ left: 0, right: 28, top: 4, bottom: 28 }),
    });
    ({ createRoot } = await import('react-dom/client'));
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  type Handler = () => void;
  function on(el: FakeElement): Record<string, Handler> {
    const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
    if (!key) throw new Error('the element has no React props');
    return (el as unknown as Record<string, Record<string, Handler>>)[key];
  }

  async function mount() {
    let carets = 0;
    const host = doc.createElement('div');
    const root = createRoot(host as unknown as HTMLElement);
    act(() => root.render(createElement(SnoopSplit, { ...props, onCaret: () => (carets += 1) })));
    const button = (text: string) =>
      findAll(host, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
    const press = async (text: string) => {
      act(() => on(button(text)).onClick());
      await Promise.resolve();
    };
    const more = () =>
      act(() =>
        on(findAll(host, (el) => el.getAttribute('aria-label') === 'Snoop options')[0]).onClick(),
      );
    const items = () =>
      findAll(host, (el) => el.getAttribute('role') === 'menuitem').map((el) => el.textContent);
    const handle = () => findAll(host, (el) => el.getAttribute('role') === 'separator')[0];
    const finding = () => findAll(host, (el) => el.getAttribute('data-find') === '').length > 0;
    const ask = (request: SnoopRequest) =>
      act(() => {
        for (const cb of heard.get(SNOOP_REQUEST_EVENT) ?? []) cb({ detail: request });
      });
    const rerender = () =>
      act(() => root.render(createElement(SnoopSplit, { ...props, onCaret: () => (carets += 1) })));
    return {
      press,
      ask,
      rerender,
      more,
      items,
      handle,
      finding,
      carets: () => carets,
      unmount: () => act(() => root.unmount()),
    };
  }

  it('stops the snoop in front with its player and hands the caret back', async () => {
    snoops([live('Tolliver'), live('Orla')], 'Orla');
    const split = await mount();
    await split.press('Stop');
    expect(fake.stops).toEqual([[1, 'Orla']]);
    expect(split.carets()).toBe(1);
    await split.press('Tolliver');
    expect(fake.selected).toEqual(['Tolliver']);
    expect(split.carets()).toBe(2);
    split.unmount();
  });

  it('closes an ended tab', async () => {
    snoops([live('Tolliver'), ended('Maren', NOW - 2 * MIN)], 'Maren');
    const split = await mount();
    await split.press('Close');
    expect(fake.closes).toEqual([[1, 'Maren']]);
    expect(fake.stops).toEqual([]);
    split.unmount();
  });

  describe('the more menu (board 02)', () => {
    it('holds Stop, Stop every snoop, Find with its keys, Open in a window and Fold', async () => {
      snoops([live('Tolliver'), live('Maren'), live('Orla')], 'Tolliver', ['Maren']);
      const split = await mount();
      split.more();
      expect(split.items()).toEqual([
        'Stop snooping Tolliver',
        'Stop every snoop',
        FIND,
        'Open in a window',
        'Fold',
      ]);
      split.unmount();
    });

    it('offers Close and Unfold for an ended tab in a folded split', async () => {
      fake.size = { share: 0.4, folded: true };
      snoops([live('Tolliver'), ended('Maren', NOW - 2 * MIN)], 'Maren');
      const split = await mount();
      split.more();
      expect(split.items()[0]).toBe('Close Maren');
      expect(split.items()[4]).toBe('Unfold');
      split.unmount();
    });

    it('sends each pick on, and hands the caret back', async () => {
      snoops([live('Tolliver'), live('Orla')], 'Tolliver');
      const split = await mount();
      split.more();
      await split.press('Stop snooping Tolliver');
      split.more();
      await split.press('Stop every snoop');
      split.more();
      await split.press('Open in a window');
      split.more();
      await split.press('Fold');
      expect(fake.stops).toEqual([
        [1, 'Tolliver'],
        [1, undefined],
      ]);
      expect(fake.windows).toEqual([1]);
      expect(fake.saves).toEqual([{ share: 0.4, folded: true }]);
      expect(split.carets()).toBeGreaterThanOrEqual(4);
      expect(split.items()).toEqual([]);
      split.unmount();
    });

    it('opens the find bar on the tab in front, unfolding first', async () => {
      fake.size = { share: 0.4, folded: true };
      snoops([live('Tolliver')], 'Tolliver');
      const split = await mount();
      expect(split.finding()).toBe(false);
      split.more();
      await split.press(FIND);
      expect(fake.saves).toEqual([{ share: 0.4, folded: false }]);
      split.unmount();
    });

    it('opens the find bar under the strip', async () => {
      snoops([live('Tolliver')], 'Tolliver');
      const split = await mount();
      split.more();
      await split.press(FIND);
      expect(split.finding()).toBe(true);
      expect(fake.saves).toEqual([]);
      split.unmount();
    });
  });

  describe('Cmd J, Cmd F and Copy (SN7)', () => {
    it('puts the caret in the tab in front', async () => {
      snoops([live('Tolliver'), live('Orla')], 'Orla');
      const split = await mount();
      split.ask('enter');
      expect(fake.carets).toEqual(['Orla']);
      expect(fake.selected).toEqual([]);
      split.unmount();
    });

    it('steps to the next tab from inside one, and round to the first', async () => {
      snoops([live('Tolliver'), live('Maren'), ended('Orla', NOW - MIN)], 'Maren');
      const split = await mount();
      split.ask('next');
      split.ask('next');
      expect(fake.selected).toEqual(['Orla', 'Tolliver']);
      expect(fake.carets).toEqual(['Orla', 'Tolliver']);
      split.unmount();
    });

    it('unfolds a folded split and puts the caret in once it shows', async () => {
      fake.size = { share: 0.4, folded: true };
      snoops([live('Tolliver')], 'Tolliver');
      const split = await mount();
      split.ask('enter');
      expect(fake.saves).toEqual([{ share: 0.4, folded: false }]);
      expect(fake.carets).toEqual([]);
      fake.size = { share: 0.4, folded: false };
      split.rerender();
      expect(fake.carets).toEqual(['Tolliver']);
      split.unmount();
    });

    it('brings the snoop window forward while the snoops sit there', async () => {
      snoops([live('Tolliver')], 'Tolliver', [], true);
      const split = await mount();
      split.ask('enter');
      split.ask('next');
      expect(fake.windows).toEqual([1, 1]);
      expect(fake.carets).toEqual([]);
      split.unmount();
    });

    it('does nothing with no snoop', async () => {
      snoops([], null);
      const split = await mount();
      split.ask('enter');
      split.ask('find');
      expect(fake.carets).toEqual([]);
      expect(fake.windows).toEqual([]);
      split.unmount();
    });

    it('opens Find on the tab in front and copies what you selected in it', async () => {
      snoops([live('Tolliver'), live('Maren')], 'Maren');
      const split = await mount();
      split.ask('find');
      expect(split.finding()).toBe(true);
      split.ask('copy');
      await Promise.resolve();
      expect(fake.copied).toEqual(['Maren selected']);
      split.unmount();
    });
  });

  describe('the line under the split', () => {
    it('folds or unfolds on a double click and tells the store', async () => {
      snoops([live('Tolliver')], 'Tolliver');
      const split = await mount();
      expect(fake.folds).toEqual([false]);
      act(() => on(split.handle()).onDoubleClick());
      expect(fake.saves).toEqual([{ share: 0.4, folded: true }]);
      split.unmount();
    });
  });
});

describe('the snoop sheet', () => {
  it('is in the stylesheet entry', () => {
    expect(indexCss).toContain("@import './snoop.css';");
  });

  it('draws the marks as the boards do', () => {
    const rule = (selector: string) =>
      snoopCss.match(
        new RegExp(`\\n${selector.replace(/[.[\]'=:()]/g, '\\$&')} \\{([^}]*)\\}`),
      )?.[1];
    expect(rule('.snoop-dot')).toMatch(/width: 8px;[\s\S]*background: var\(--success\);/);
    expect(rule('.snoop-tab.is-ended .snoop-dot')).toContain(
      'box-shadow: inset 0 0 0 1.25px var(--tertiary);',
    );
    expect(rule('.snoop-tab.is-unread::after')).toMatch(
      /width: 6px;[\s\S]*background: var\(--accent\);/,
    );
    expect(rule(".snoop-tab[aria-selected='true']")).toContain('background: var(--selrow);');
    expect(rule('.snoop-strip')).toContain('height: var(--band);');
    expect(rule('.snoop-body')).toContain('padding: 0 16px 6px;');
    expect(rule('.snoop')).toContain('border-bottom: 1px solid var(--sep);');
  });
});
