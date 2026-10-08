import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SnoopTab } from '../ipc/snoop';
import type { Snoops } from '../stores/session/snoopStore';
import indexCss from '../styles/index.css?raw';
import snoopCss from '../styles/snoop.css?raw';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { SnoopSplit } from './SnoopSplit';
import { endedLine, tabTitle } from './snoopLine';

// The snoop split of boards 01 and 03 of the Snoop review. The snoop
// store, the saved share and the minute clock are faked, and each snoop
// terminal stands in as a plain element that names its player, so what
// shows is what the split draws.

const fake = vi.hoisted(() => ({
  snoops: { tabs: [], windowed: false, selected: null, unread: new Set() } as unknown as Snoops,
  share: 0.4,
  selected: [] as string[],
  stops: [] as unknown[],
  closes: [] as unknown[],
}));

const NOW = 1_800_000_000_000;
const MIN = 60_000;

vi.mock('../stores/session/snoopStore', () => ({
  useSnoops: () => fake.snoops,
  selectSnoop: (name: string) => fake.selected.push(name),
}));
vi.mock('../stores/config/snoopShareStore', () => ({ useSnoopShare: () => fake.share }));
vi.mock('./sessionLine', async (actual) => ({
  ...(await actual<typeof import('./sessionLine')>()),
  useMinuteClock: () => NOW,
}));
vi.mock('../ipc/snoop', () => ({
  snoopStop: async (session?: number, name?: string) => void fake.stops.push([session, name]),
  snoopClose: async (session?: number, name?: string) => void fake.closes.push([session, name]),
}));
vi.mock('../terminal/SnoopTerminal', () => ({
  SnoopTerminal: ({ name, shown }: { name: string; shown: boolean }) =>
    createElement('div', { 'data-term': name, hidden: !shown }),
}));

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
  fake.share = 0.4;
  fake.selected.length = 0;
  fake.stops.length = 0;
  fake.closes.length = 0;
});

describe('the snoop split', () => {
  it('draws nothing with no snoop, or while the snoops sit in their window', () => {
    snoops([], null);
    expect(draw()).toBe('');
    snoops([live('Tolliver')], 'Tolliver', [], true);
    expect(draw()).toBe('');
  });

  it('opens at the saved share with the eye, the tab and Stop (board 01)', () => {
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
      '<div class="snoop-end"><button type="button" class="snoop-btn">Stop</button></div>',
    );
    expect(html).toContain('<div class="snoop-body"><div data-term="Tolliver"></div></div>');

    fake.share = 0.25;
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
      '<span class="snoop-meta">Ended 2 min ago</span><button type="button" class="snoop-btn">Close</button>',
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

  beforeAll(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener() {},
      removeEventListener() {},
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
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
      on(button(text)).onClick();
      await Promise.resolve();
    };
    return { press, carets: () => carets, unmount: () => act(() => root.unmount()) };
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
