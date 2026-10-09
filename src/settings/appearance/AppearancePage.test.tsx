import { act, createElement, useState } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import tokyoNight from '../../../fixtures/themes/tokyonight_night.conf?raw';
import type { SystemFontEntry, UiConfig } from '../../ipc/uiConfig';
import type { XtermPalette } from '../../theme/themes';
import { FakeDocument, findAll, type FakeElement, type FakeNode } from '../../test/fakeDom';
import type { AppearancePage as AppearancePageType } from './AppearancePage';
import type { AccessibilityPage as AccessibilityPageType } from '../accessibility/AccessibilityPage';

// The Font select waits on fonts_list, the one slow read on this page.
// The first read of a launch takes a moment, so the page must draw the
// gallery and the Font select without it and add the installed fonts
// when the list comes in. The page reads the list in an effect, so this
// test mounts it for real, into the small stand in for the DOM in
// src/test/fakeDom.ts.

const fonts = vi.hoisted(() => {
  let resolve: (list: SystemFontEntry[]) => void = () => {};
  const pending = new Promise<SystemFontEntry[]>((r) => {
    resolve = r;
  });
  return { pending, resolve: (list: SystemFontEntry[]) => resolve(list) };
});

const invoke = vi.hoisted(() =>
  vi.fn((cmd: string) => (cmd === 'fonts_list' ? fonts.pending : Promise.resolve(undefined))),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

// The game color fit runs in a worker. Here it answers when a test says.
const fitting = vi.hoisted(() => {
  const asked: XtermPalette[] = [];
  let answer: (fitted: Partial<XtermPalette> | null) => void = () => {};
  const fitOffThread = vi.fn(
    (palette: XtermPalette) =>
      new Promise<Partial<XtermPalette> | null>((resolve) => {
        asked.push(palette);
        answer = resolve;
      }),
  );
  return { asked, fitOffThread, answer: (fitted: Partial<XtermPalette>) => answer(fitted) };
});
vi.mock('../../theme/fitOffThread', () => ({ fitOffThread: fitting.fitOffThread }));
// The selected session's day or night, which a test sets. Like the real
// store it says nothing until something starts it.
const daylight = vi.hoisted(() => ({
  now: null as 'day' | 'night' | null,
  started: false,
}));
vi.mock('../../stores/session/daylightStore', () => ({
  getDaylight: () => (daylight.started ? daylight.now : null),
  startDaylightStore: () => {
    daylight.started = true;
  },
  subscribeDaylight: () => {
    daylight.started = true;
    return () => undefined;
  },
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// ── The page ─────────────────────────────────────────────────────────

const doc = new FakeDocument();
let AppearancePage: typeof AppearancePageType;
let AccessibilityPage: typeof AccessibilityPageType;
let createRoot: typeof import('react-dom/client').createRoot;
let normalizeUiConfig: typeof import('../../ipc/uiConfig').normalizeUiConfig;
let BUILTIN_THEMES: typeof import('../../theme/themes').BUILTIN_THEMES;

// A dark OS, with Increase contrast only where a test turns it on.
let moreContrast = false;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    matchMedia: (query: string) => ({
      matches: query.includes('contrast') ? moreContrast : true,
      addEventListener() {},
      removeEventListener() {},
    }),
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  // React DOM reads navigator.userAgent when it loads. Node 20, the CI
  // version, has no navigator of its own.
  vi.stubGlobal('navigator', { userAgent: 'node' });
  // React DOM checks for a DOM once, when it loads, so it loads now.
  ({ createRoot } = await import('react-dom/client'));
  ({ AppearancePage } = await import('./AppearancePage'));
  ({ AccessibilityPage } = await import('../accessibility/AccessibilityPage'));
  ({ normalizeUiConfig } = await import('../../ipc/uiConfig'));
  ({ BUILTIN_THEMES } = await import('../../theme/themes'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const CURRENT = '"PT Mono", Menlo, monospace';

function config(): UiConfig {
  return normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: CURRENT,
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  });
}

/** The Font select's options as label and value. */
function fontOptions(root: FakeNode): { label: string; value: string }[] {
  const [row] = findAll(root, (el) => el.getAttribute('data-st-anchor') === 'font');
  const [select] = findAll(row, (el) => el.nodeName === 'SELECT');
  return select.options.map((o) => ({ label: o.textContent, value: o.value }));
}

describe('AppearancePage', () => {
  it('draws the gallery and the Font select before fonts_list answers', async () => {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    const props = {
      target: { group: 'appearance' },
      navSeq: 0,
      config: config(),
      setConfig: () => undefined,
      onError: () => undefined,
      pathB: false,
      navigate: () => undefined,
      setLeaveGuard: () => undefined,
    } as const;

    await act(async () => {
      root.render(createElement(AppearancePage, props));
    });

    // fonts_list is still out.
    expect(invoke).toHaveBeenCalledWith('fonts_list');
    const radios = findAll(container, (el) => el.getAttribute('type') === 'radio');
    expect(radios.length).toBe(BUILTIN_THEMES.length);
    expect(fontOptions(container)).toEqual([
      { label: 'PT Mono', value: CURRENT },
      { label: 'JetBrains Mono', value: '"JetBrainsMono Bundled", Menlo, monospace' },
    ]);

    await act(async () => {
      fonts.resolve([
        { family: 'Helvetica', monospace: false },
        { family: 'JetBrains Mono', monospace: true },
        { family: 'Menlo', monospace: true },
        { family: 'PT Mono', monospace: true },
        { family: 'Times', monospace: false },
      ]);
      await fonts.pending;
    });

    // The monospace families join the bundled ones. The proportional
    // ones stay out, and your font keeps its exact list.
    expect(fontOptions(container)).toEqual([
      { label: 'JetBrains Mono', value: '"JetBrainsMono Bundled", Menlo, monospace' },
      { label: 'Menlo', value: '"Menlo", Menlo, monospace' },
      { label: 'PT Mono', value: CURRENT },
    ]);
    expect(findAll(container, (el) => el.getAttribute('type') === 'radio').length).toBe(
      BUILTIN_THEMES.length,
    );

    await act(async () => {
      root.unmount();
    });
  });

  it('describes and credits the theme on screen under the gallery', async () => {
    const caption = async (ui: UiConfig) => {
      const container = doc.createElement('div');
      doc.body.appendChild(container);
      const root = createRoot(container as unknown as HTMLElement);
      await act(async () => {
        root.render(
          createElement(AppearancePage, {
            target: { group: 'appearance' },
            navSeq: 0,
            config: ui,
            setConfig: () => undefined,
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const found = findAll(container, (el) =>
        (el.getAttribute('class') ?? '').split(' ').includes('st-theme-caption'),
      );
      const shown = found.map((el) => ({
        text: el.textContent,
        className: el.getAttribute('class'),
        after: el.parentNode?.childNodes[el.parentNode.childNodes.indexOf(el) - 1]?.nodeName,
      }));
      await act(async () => {
        root.unmount();
      });
      return shown;
    };

    expect(await caption(config())).toEqual([
      {
        text:
          'Arctic palette. Polar nights base, frost accents. ' +
          'Its colors come from Nord by Sven Greb, under the MIT license.',
        className: 'st-meta st-theme-caption',
        after: 'FIELDSET',
      },
    ]);

    const dusk = { id: 'dusk', label: 'Dusk', description: 'Low light.', xterm: {}, chrome: {} };
    const custom = await caption({ ...config(), theme: 'dusk', custom_themes: [dusk] });
    expect(custom.map((c) => c.text)).toEqual(['Low light.']);
    const blank = { ...dusk, description: '' };
    expect(await caption({ ...config(), theme: 'dusk', custom_themes: [blank] })).toEqual([]);
  });

  it('marks the theme a retired id shows as chosen, in the gallery and the selects', async () => {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    // The window above reads every media query as a match, so the OS is
    // dark and the gallery shows the dark theme.
    const ui = {
      ...config(),
      theme: 'vellum',
      follow_system_appearance: true,
      light_theme: 'vellum',
      dark_theme: 'one-dark',
    };
    await act(async () => {
      root.render(
        createElement(AppearancePage, {
          target: { group: 'appearance' },
          navSeq: 0,
          config: ui,
          setConfig: () => undefined,
          onError: () => undefined,
          pathB: false,
          navigate: () => undefined,
          setLeaveGuard: () => undefined,
        }),
      );
    });
    const isChecked = (el: FakeElement) => (el as unknown as { checked?: boolean }).checked;
    const radios = findAll(container, (el) => el.getAttribute('type') === 'radio');
    expect(radios.filter(isChecked).map((el) => el.value)).toEqual(['one-half-dark']);
    const select = (anchor: string) => {
      const [row] = findAll(container, (el) => el.getAttribute('data-st-anchor') === anchor);
      const [found] = findAll(row, (el) => el.nodeName === 'SELECT');
      return found;
    };
    const isSelected = (el: FakeElement) => (el as unknown as { selected?: boolean }).selected;
    for (const [anchor, saved, shown] of [
      ['light-theme', 'vellum', 'rubric'],
      ['dark-theme', 'one-dark', 'one-half-dark'],
    ]) {
      const options = select(anchor).options;
      expect(
        options.filter(isSelected).map((o) => o.value),
        anchor,
      ).toEqual([shown]);
      // The retired theme gets no option of its own.
      expect(
        options.map((o) => o.value),
        anchor,
      ).not.toContain(saved);
    }
    await act(async () => {
      root.unmount();
    });
  });

  it('draws Collapse repeated lines after the theme colors switch, off until you turn it on', async () => {
    const collapseSwitch = async (cfg: UiConfig) => {
      const container = doc.createElement('div');
      doc.body.appendChild(container);
      const root = createRoot(container as unknown as HTMLElement);
      await act(async () => {
        root.render(
          createElement(AppearancePage, {
            target: { group: 'appearance' },
            navSeq: 0,
            config: cfg,
            setConfig: () => undefined,
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const anchors = findAll(container, (el) => el.getAttribute('data-st-anchor') !== null).map(
        (el) => el.getAttribute('data-st-anchor'),
      );
      const [row] = findAll(
        container,
        (el) => el.getAttribute('data-st-anchor') === 'collapse-repeats',
      );
      const [input] = findAll(row, (el) => el.getAttribute('role') === 'switch');
      const checked = (input as unknown as { checked: boolean }).checked;
      await act(async () => {
        root.unmount();
      });
      return { label: row.textContent, checked, anchors };
    };

    const off = await collapseSwitch(config());
    expect(off.label).toContain('Collapse repeated lines');
    expect(off.checked).toBe(false);
    // It follows the theme colors switch, since the rows that make the
    // game easier to see left for Accessibility.
    const at = off.anchors.indexOf('collapse-repeats');
    expect(off.anchors[at - 1]).toBe('theme-colors');
    for (const gone of ['fit-game-colors', 'color-vision', 'readable-highlights', 'blink-text']) {
      expect(off.anchors).not.toContain(gone);
    }
    const on = await collapseSwitch({ ...config(), collapse_repeats: true });
    expect(on.checked).toBe(true);
  });

  /** One row under Collapse repeated lines as the page draws it. */
  interface CollapseRow {
    text: string;
    /** The row waits, in the tertiary tone with its control faded. */
    waiting: boolean;
    pressed: string[];
    /** Whether each segment, Collapse then Show every line, is off. */
    off: boolean[];
    segments: FakeElement[];
  }

  /** Call an element's click handler. The fake DOM sends no events, so
   *  read the handler from the props React keeps on the element. */
  function press(el: FakeElement) {
    const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
    const props = key
      ? (el as unknown as Record<string, { onClick?: (e: unknown) => void }>)[key]
      : undefined;
    if (!props?.onClick) throw new Error('the segment has no click handler');
    props.onClick({ preventDefault() {}, stopPropagation() {} });
  }

  /** Draw the page with `cfg` on a link to `anchor`, or bare, and read
   *  the anchors, the two rows under Collapse repeated lines, and the
   *  config a press of `then` saves. */
  async function collapseRows(
    cfg: UiConfig,
    anchor?: string,
    then?: (rows: { fights: CollapseRow; attacks: CollapseRow }) => FakeElement,
  ) {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    let saved: UiConfig | null = null;
    await act(async () => {
      root.render(
        createElement(AppearancePage, {
          target: anchor
            ? { group: 'appearance', section: 'text', anchor }
            : { group: 'appearance' },
          navSeq: 0,
          config: cfg,
          setConfig: (next) => {
            saved = next(cfg);
          },
          onError: () => undefined,
          pathB: false,
          navigate: () => undefined,
          setLeaveGuard: () => undefined,
        }),
      );
    });
    const anchors = findAll(container, (el) => el.getAttribute('data-st-anchor') !== null).map(
      (el) => el.getAttribute('data-st-anchor'),
    );
    const read = (at: string): CollapseRow | null => {
      const [row] = findAll(container, (el) => el.getAttribute('data-st-anchor') === at);
      if (!row) return null;
      const segments = findAll(row, (el) => el.nodeName === 'BUTTON');
      return {
        text: row.textContent,
        waiting: (row.getAttribute('class') ?? '').split(' ').includes('is-disabled'),
        pressed: segments
          .filter((el) => el.getAttribute('aria-pressed') === 'true')
          .map((el) => el.textContent),
        off: segments.map((el) => el.hasAttribute('disabled')),
        segments,
      };
    };
    const fights = read('collapse-fights');
    const attacks = read('collapse-attacks');
    if (then && fights && attacks) {
      const segment = then({ fights, attacks });
      await act(async () => {
        press(segment);
      });
    }
    await act(async () => {
      root.unmount();
    });
    return { anchors, fights, attacks, saved: saved as UiConfig | null };
  }

  it('shows In a fight and Attack lines under Collapse repeated lines while it is on', async () => {
    // Off, the rows stay away.
    const off = await collapseRows(config());
    expect(off.fights).toBeNull();
    expect(off.attacks).toBeNull();
    expect(off.anchors).not.toContain('collapse-fights');

    // On, they follow it. In a fight starts on Collapse, and Attack lines
    // on Show every line.
    const on = await collapseRows({ ...config(), collapse_repeats: true });
    const at = on.anchors.indexOf('collapse-repeats');
    expect(on.anchors.slice(at, at + 3)).toEqual([
      'collapse-repeats',
      'collapse-fights',
      'collapse-attacks',
    ]);
    expect(on.fights?.text).toContain('In a fight');
    expect(on.fights?.text).toContain('Every line that arrives while you are fighting.');
    expect(on.fights?.pressed).toEqual(['Collapse']);
    expect(on.fights?.waiting).toBe(false);
    expect(on.attacks?.text).toContain('Attack lines');
    expect(on.attacks?.text).toContain('Each hit and miss the game shows you, in a fight or not.');
    expect(on.attacks?.pressed).toEqual(['Show every line']);
    expect(on.attacks?.waiting).toBe(false);
    expect(on.attacks?.off).toEqual([false, false]);

    const chosen = await collapseRows({
      ...config(),
      collapse_repeats: true,
      collapse_attack_lines: true,
    });
    expect(chosen.attacks?.pressed).toEqual(['Collapse']);
  });

  it('turns Attack lines off and says why while a fight shows every line', async () => {
    for (const attacks of [false, true]) {
      const drawn = await collapseRows({
        ...config(),
        collapse_repeats: true,
        collapse_fight_lines: false,
        collapse_attack_lines: attacks,
      });
      expect(drawn.fights?.pressed).toEqual(['Show every line']);
      expect(drawn.fights?.waiting).toBe(false);
      // Attack lines show every line then, whatever their own choice.
      expect(drawn.attacks?.pressed).toEqual(['Show every line']);
      expect(drawn.attacks?.waiting).toBe(true);
      expect(drawn.attacks?.off).toEqual([true, true]);
      expect(drawn.attacks?.text).toContain('Attack lines show every line while In a fight does.');
    }
  });

  it('shows both rows waiting on a link to one while Collapse repeated lines is off', async () => {
    for (const anchor of ['collapse-fights', 'collapse-attacks']) {
      const drawn = await collapseRows(config(), anchor);
      expect(drawn.anchors).toContain(anchor);
      for (const row of [drawn.fights, drawn.attacks]) {
        expect(row?.waiting).toBe(true);
        expect(row?.off).toEqual([true, true]);
        expect(row?.text).toContain('Turn on Collapse repeated lines to choose.');
      }
    }
    // Waiting, Attack lines still follows In a fight, which shows every
    // line, whatever Attack lines saved.
    const whole = await collapseRows(
      { ...config(), collapse_fight_lines: false, collapse_attack_lines: true },
      'collapse-attacks',
    );
    expect(whole.fights?.pressed).toEqual(['Show every line']);
    expect(whole.attacks?.pressed).toEqual(['Show every line']);
  });

  it('gives Panel text a section of its own after Terminal text, with Font and Size', async () => {
    // The installed fonts are in, as the first test leaves them.
    await act(async () => {
      fonts.resolve([
        { family: 'JetBrains Mono', monospace: true },
        { family: 'Menlo', monospace: true },
        { family: 'PT Mono', monospace: true },
      ]);
      await fonts.pending;
    });
    const panelRow = async (anchor: string, cfg: UiConfig, pick?: string) => {
      const container = doc.createElement('div');
      doc.body.appendChild(container);
      const root = createRoot(container as unknown as HTMLElement);
      let saved: UiConfig | null = null;
      await act(async () => {
        root.render(
          createElement(AppearancePage, {
            target: { group: 'appearance' },
            navSeq: 0,
            config: cfg,
            setConfig: (next) => {
              saved = next(cfg);
            },
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const anchors = findAll(container, (el) => el.getAttribute('data-st-anchor') !== null).map(
        (el) => el.getAttribute('data-st-anchor'),
      );
      const [section] = findAll(
        container,
        (el) => el.getAttribute('data-st-anchor') === 'panel-text',
      );
      const [title] = findAll(section, (el) => el.nodeName === 'H2');
      const [row] = findAll(section, (el) => el.getAttribute('data-st-anchor') === anchor);
      const [select] = findAll(row, (el) => el.nodeName === 'SELECT');
      const options = select.options.map((o) => ({ label: o.textContent, value: o.value }));
      // React marks the shown option selected.
      const value = select.options.find(
        (o) => (o as unknown as { selected?: boolean }).selected,
      )?.value;
      const font = fontOptions(container);
      if (pick !== undefined) {
        // The fake DOM sends no events, so call the handler React keeps.
        const key = Object.keys(select).find((k) => k.startsWith('__reactProps$'));
        const props = key
          ? (select as unknown as Record<string, { onChange?: (e: unknown) => void }>)[key]
          : undefined;
        await act(async () => {
          props?.onChange?.({ target: { value: pick } });
        });
      }
      await act(async () => {
        root.unmount();
      });
      return {
        anchors,
        title: title.textContent,
        label: row.textContent,
        options,
        font,
        value,
        saved: saved as UiConfig | null,
      };
    };

    const same = await panelRow('panel-font', config(), 'system');
    // Terminal text keeps Font, Size, and the rest. Panel text follows it,
    // with its Font, then its Size, before Advanced.
    expect(same.anchors.indexOf('size')).toBe(same.anchors.indexOf('font') + 1);
    expect(same.anchors.slice(same.anchors.indexOf('panel-text'))).toEqual([
      'panel-text',
      'panel-font',
      'panel-size',
      'advanced',
    ]);
    expect(same.anchors.indexOf('panel-text')).toBeGreaterThan(
      same.anchors.indexOf('collapse-repeats'),
    );
    expect(same.title).toBe('Panel text');
    expect(same.label).toContain('Font');
    expect(same.label).not.toContain('Panel font');
    expect(same.label).toContain('Every pane and the status line under the terminal draw in it.');
    expect(same.value).toBe('');
    // As designed, the default, the terminal font, the system font, then
    // the list Font offers.
    expect(same.options.slice(0, 3)).toEqual([
      { label: 'As designed', value: '' },
      { label: 'Same as terminal', value: 'terminal' },
      { label: 'System font', value: 'system' },
    ]);
    expect(same.options.slice(3).map((o) => o.label)).toEqual(same.font.map((o) => o.label));
    expect(same.options.slice(3).map((o) => o.label)).toEqual([
      'JetBrains Mono',
      'Menlo',
      'PT Mono',
    ]);
    expect(same.saved?.panel_font).toBe('system');
    expect(same.saved?.font_family).toBe(CURRENT);

    const system = await panelRow('panel-font', { ...config(), panel_font: 'system' });
    expect(system.value).toBe('system');
    const terminal = await panelRow('panel-font', config(), 'terminal');
    expect(terminal.saved?.panel_font).toBe('terminal');
    const designed = await panelRow('panel-font', { ...config(), panel_font: 'terminal' }, '');
    expect(designed.value).toBe('terminal');
    expect(designed.saved?.panel_font).toBe('');
    const menlo = '"Menlo", Menlo, monospace';
    const picked = await panelRow('panel-font', { ...config(), panel_font: menlo });
    expect(picked.value).toBe(menlo);
    expect(picked.options.filter((o) => o.value === menlo)).toEqual([
      { label: 'Menlo', value: menlo },
    ]);

    // Size starts at 12, offers Same as terminal and the sizes the
    // terminal Size offers, and saves your pick.
    const size = await panelRow('panel-size', config(), '0');
    expect(size.label).toContain('Size');
    expect(size.label).toContain('The headers, the rows, and the status line grow with it.');
    expect(size.value).toBe('12');
    expect(size.options.map((o) => o.label)).toEqual([
      'Same as terminal',
      '11 pt',
      '12 pt',
      '13 pt',
      '14 pt',
      '15 pt',
      '16 pt',
      '18 pt',
    ]);
    expect(size.options[0].value).toBe('0');
    expect(size.saved?.panel_font_size).toBe(0);
    expect(size.saved?.font_size).toBe(14);
    const following = await panelRow('panel-size', { ...config(), panel_font_size: 0 }, '16');
    expect(following.value).toBe('0');
    expect(following.saved?.panel_font_size).toBe(16);
    const own = await panelRow('panel-size', { ...config(), panel_font_size: 20 });
    expect(own.value).toBe('20');
    expect(own.options.at(-1)).toEqual({ label: '20 pt', value: '20' });
  });

  /** The page on `start`, under the config the Settings window keeps.
   *  `shown()` is that config now, and `leave()` moves the window to
   *  another page. */
  async function openPage(start: UiConfig) {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    let shown = start;
    function Host({ page }: { page: boolean }) {
      const [cfg, setCfg] = useState<UiConfig | null>(shown);
      if (cfg) shown = cfg;
      if (!page) return null;
      return createElement(AppearancePage, {
        target: { group: 'appearance' },
        navSeq: 0,
        config: cfg,
        setConfig: (next) => setCfg((c) => next(c)),
        onError: () => undefined,
        pathB: false,
        navigate: () => undefined,
        setLeaveGuard: () => undefined,
      });
    }
    await act(async () => {
      root.render(createElement(Host, { page: true }));
    });
    return {
      container,
      shown: () => shown,
      leave: () =>
        act(async () => {
          root.render(createElement(Host, { page: false }));
        }),
      close: () =>
        act(async () => {
          root.unmount();
        }),
    };
  }

  async function importTokyoNight(container: FakeNode) {
    const [input] = findAll(container, (el) => el.getAttribute('type') === 'file');
    const key = Object.keys(input).find((k) => k.startsWith('__reactProps$')) ?? '';
    const props = (input as unknown as Record<string, { onChange: (e: unknown) => void }>)[key];
    await act(async () => {
      props.onChange({
        target: {
          files: [{ name: 'tokyonight_night.conf', text: async () => tokyoNight }],
          value: '',
        },
      });
    });
  }

  it('adds an imported theme at once and keeps its fit once the fit answers', async () => {
    fitting.asked.length = 0;
    const page = await openPage(config());
    await importTokyoNight(page.container);
    // The theme is in and on screen before the fit answers. The built
    // in Tokyo Night holds its id.
    const [theme] = page.shown().custom_themes;
    expect(theme.id).toBe('tokyo-night-2');
    expect(page.shown().theme).toBe(theme.id);
    expect(theme).not.toHaveProperty('fitted');
    expect(fitting.asked).toHaveLength(1);
    expect(fitting.asked[0].background).toBe(theme.xterm.background);
    await act(async () => {
      fitting.answer({ red: '#f8809b', brightBlack: '#6d7498' });
    });
    expect(page.shown().custom_themes[0].fitted).toEqual({
      red: '#f8809b',
      brightBlack: '#6d7498',
    });
    expect(page.shown().custom_themes[0].xterm).toEqual(theme.xterm);
    await page.close();
  });

  it('keeps the fit of an imported theme that answers after you leave the page', async () => {
    fitting.asked.length = 0;
    const page = await openPage(config());
    await importTokyoNight(page.container);
    expect(fitting.asked).toHaveLength(1);
    await page.leave();
    await act(async () => {
      fitting.answer({ red: '#f8809b' });
    });
    expect(page.shown().custom_themes[0].fitted).toEqual({ red: '#f8809b' });
    await page.close();
  });

  it('fits each custom theme that keeps no fit once the page opens', async () => {
    fitting.asked.length = 0;
    const triad = BUILTIN_THEMES.find((t) => t.id === 'triad');
    const custom = (
      id: string,
      xterm: Record<string, string>,
      fitted?: Record<string, string>,
    ) => ({
      id,
      label: id,
      description: '',
      xterm,
      chrome: {},
      ...(fitted && { fitted }),
    });
    const dusk = custom('dusk', { background: '#1a1b26', foreground: '#c0caf5' });
    const start = {
      ...config(),
      custom_themes: [
        dusk,
        // Kept its fit already.
        custom('paper', { background: '#f7f4ee', foreground: '#2a2a2a' }, { red: '#a8322c' }),
        // Passes every check, so a fit would move nothing.
        custom('calm', { ...(triad?.xterm as unknown as Record<string, string>) }),
      ],
    };
    const page = await openPage(start);
    expect(fitting.asked).toHaveLength(1);
    expect(fitting.asked[0].background).toBe('#1a1b26');
    await act(async () => {
      fitting.answer({ red: '#cb7b74' });
    });
    const kept = page.shown().custom_themes;
    expect(kept.map((t) => t.fitted)).toEqual([{ red: '#cb7b74' }, { red: '#a8322c' }, undefined]);
    await page.close();
  });

  it('keeps an imported theme off the id of a retired theme', async () => {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    let shown: UiConfig = { ...config(), dark_theme: 'one-dark' };
    function Host() {
      const [cfg, setCfg] = useState<UiConfig | null>(shown);
      if (cfg) shown = cfg;
      return createElement(AppearancePage, {
        target: { group: 'appearance' },
        navSeq: 0,
        config: cfg,
        setConfig: (next) => setCfg((c) => next(c)),
        onError: () => undefined,
        pathB: false,
        navigate: () => undefined,
        setLeaveGuard: () => undefined,
      });
    }
    await act(async () => {
      root.render(createElement(Host));
    });
    const [input] = findAll(container, (el) => el.getAttribute('type') === 'file');
    const key = Object.keys(input).find((k) => k.startsWith('__reactProps$')) ?? '';
    const props = (input as unknown as Record<string, { onChange: (e: unknown) => void }>)[key];
    // A One Dark file you import. Your saved One Dark still shows One
    // Half Dark, not the import.
    const oneDark = tokyoNight.replace('## name: Tokyo Night', '## name: One Dark');
    await act(async () => {
      props.onChange({
        target: { files: [{ name: 'one_dark.conf', text: async () => oneDark }], value: '' },
      });
    });
    expect(shown.custom_themes.map((t) => t.id)).toEqual(['one-dark-2']);
    expect(shown.dark_theme).toBe('one-dark');
    await act(async () => {
      root.unmount();
    });
  });

  it('saves the choice you press in each row', async () => {
    const on = { ...config(), collapse_repeats: true };
    const fights = await collapseRows(on, undefined, (rows) => rows.fights.segments[1]);
    expect(fights.saved?.collapse_fight_lines).toBe(false);
    expect(fights.saved?.collapse_attack_lines).toBe(false);
    const attacks = await collapseRows(on, undefined, (rows) => rows.attacks.segments[0]);
    expect(attacks.saved?.collapse_attack_lines).toBe(true);
    expect(attacks.saved?.collapse_fight_lines).toBe(true);
  });

  /** Draw the page with `cfg`, read the Theme card's rows after the
   *  gallery, and the config `then` saves when it acts on the page. */
  async function themeCard(cfg: UiConfig, then?: (container: FakeNode) => void, anchor?: string) {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    let saved: UiConfig | null = null;
    await act(async () => {
      root.render(
        createElement(AppearancePage, {
          target: anchor
            ? { group: 'appearance', section: 'theme', anchor }
            : { group: 'appearance' },
          navSeq: 0,
          config: cfg,
          setConfig: (next) => {
            saved = next(cfg);
          },
          onError: () => undefined,
          pathB: false,
          navigate: () => undefined,
          setLeaveGuard: () => undefined,
        }),
      );
    });
    const anchors = findAll(container, (el) => el.getAttribute('data-st-anchor') !== null).map(
      (el) => el.getAttribute('data-st-anchor'),
    );
    const [row] = findAll(container, (el) => el.getAttribute('data-st-anchor') === 'switch-themes');
    const segments = findAll(row, (el) => el.nodeName === 'BUTTON');
    const pressed = segments
      .filter((el) => el.getAttribute('aria-pressed') === 'true')
      .map((el) => el.textContent);
    // Each pair select's options and the one it shows.
    const selects = new Map<string, { options: string[]; chosen: string | undefined }>();
    for (const at of findAll(container, (el) => el.getAttribute('data-st-anchor') !== null)) {
      const [select] = findAll(at, (el) => el.nodeName === 'SELECT');
      if (!select) continue;
      selects.set(at.getAttribute('data-st-anchor') ?? '', {
        options: select.options.map((o) => o.value),
        chosen: select.options.find((o) => (o as unknown as { selected?: boolean }).selected)
          ?.value,
      });
    }
    const drawn = {
      anchors: anchors.slice(anchors.indexOf('switch-themes'), anchors.indexOf('text')),
      text: row.textContent,
      segments: segments.map((el) => el.textContent),
      pressed,
      options: (anchor: string) => selects.get(anchor)?.options,
      chosen: (anchor: string) => selects.get(anchor)?.chosen,
    };
    if (then) {
      await act(async () => {
        then(container);
      });
    }
    await act(async () => {
      root.unmount();
    });
    return { ...drawn, saved: saved as UiConfig | null };
  }

  /** The segment of Switch themes that reads `label`. */
  const segment = (container: FakeNode, label: string) =>
    findAll(
      container,
      (el) => el.getAttribute('class') === 'st-seg-item' && el.textContent === label,
    )[0];

  /** Call the onChange React keeps on an element. */
  function change(el: FakeElement, event: unknown) {
    const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
    (el as unknown as Record<string, { onChange: (e: unknown) => void }>)[key].onChange(event);
  }

  const gameConfig = (): UiConfig => ({
    ...config(),
    theme_follow: 'game',
    day_theme: 'gruvbox',
    night_theme: 'obsidian-ember',
  });

  it('shows the pair each Switch themes mode switches between, and Off shows neither', async () => {
    const off = await themeCard(config());
    expect(off.segments).toEqual(['Off', 'With the system', 'With the game']);
    expect(off.pressed).toEqual(['Off']);
    expect(off.anchors).toEqual(['switch-themes']);

    const system = await themeCard({ ...config(), follow_system_appearance: true });
    expect(system.pressed).toEqual(['With the system']);
    expect(system.anchors).toEqual(['switch-themes', 'light-theme', 'dark-theme']);
    expect(system.text).toContain(
      'Vosh switches between your light and dark theme when your system does.',
    );

    // While Increase contrast shows High Contrast, the line says why a
    // pick does not show yet.
    moreContrast = true;
    try {
      const more = await themeCard({ ...config(), follow_system_appearance: true });
      expect(more.text).toContain(
        "Your system is set to increase contrast, so High Contrast shows. Your pick shows once that's off.",
      );
      expect(more.text).not.toContain('Vosh switches between');
      const offMore = await themeCard(config());
      expect(offMore.text).not.toContain('increase contrast');
    } finally {
      moreContrast = false;
    }

    const game = await themeCard(gameConfig());
    expect(game.pressed).toEqual(['With the game']);
    expect(game.anchors).toEqual(['switch-themes', 'day-theme', 'night-theme']);
    expect(game.text).toContain("Turns at the game's dawn and dusk, about every 6 minutes.");
    expect(game.chosen('day-theme')).toBe('gruvbox');
    expect(game.chosen('night-theme')).toBe('obsidian-ember');
    // A link to a row of a pair shows that pair in any mode, so search
    // lands on it.
    expect((await themeCard(config(), undefined, 'night-theme')).anchors).toEqual([
      'switch-themes',
      'day-theme',
      'night-theme',
    ]);
    expect((await themeCard(gameConfig(), undefined, 'light-theme')).anchors).toEqual([
      'switch-themes',
      'light-theme',
      'dark-theme',
      'day-theme',
      'night-theme',
    ]);
    // Day and Night list every theme, light or dark.
    for (const anchor of ['day-theme', 'night-theme']) {
      const listed = game.options(anchor);
      expect(listed, anchor).toContain('rubric');
      expect(listed, anchor).toContain('obsidian-ember');
      expect(listed, anchor).toHaveLength(BUILTIN_THEMES.length);
    }
  });

  it('starts both slots on the theme showing when you choose With the game', async () => {
    const chose = await themeCard(config(), (c) => press(segment(c, 'With the game')));
    expect(chose.saved).toMatchObject({
      theme_follow: 'game',
      follow_system_appearance: false,
      theme: 'nord',
      day_theme: 'nord',
      night_theme: 'nord',
    });
    // A slot you filled before keeps your pick.
    const kept = await themeCard({ ...config(), night_theme: 'obsidian-ember' }, (c) =>
      press(segment(c, 'With the game')),
    );
    expect(kept.saved).toMatchObject({ day_theme: 'nord', night_theme: 'obsidian-ember' });
  });

  it('keeps your theme when you choose With the game after the game said', async () => {
    // A window that never followed the game until now.
    daylight.started = false;
    daylight.now = 'night';
    const system: UiConfig = {
      ...config(),
      theme_follow: 'system',
      follow_system_appearance: true,
      dark_theme: 'tokyo-night',
    };
    const game = await themeCard(system, (c) => press(segment(c, 'With the game')));
    expect(game.saved).toMatchObject({
      theme_follow: 'game',
      theme: 'nord',
      day_theme: 'tokyo-night',
      night_theme: 'tokyo-night',
    });
    const off = await themeCard(game.saved as UiConfig, (c) => press(segment(c, 'Off')));
    expect(off.saved).toMatchObject({ theme_follow: 'off', theme: 'nord' });
    daylight.now = null;
  });

  it('saves Off as follow system appearance off', async () => {
    const off = await themeCard({ ...config(), follow_system_appearance: true }, (c) =>
      press(segment(c, 'Off')),
    );
    expect(off.saved).toMatchObject({ theme_follow: 'off', follow_system_appearance: false });
    const system = await themeCard(gameConfig(), (c) => press(segment(c, 'With the system')));
    expect(system.saved).toMatchObject({
      theme_follow: 'system',
      follow_system_appearance: true,
    });
  });

  it('fills the slot showing with a pick in the gallery while it follows the game', async () => {
    daylight.now = 'night';
    const radio = (c: FakeNode, id: string) =>
      findAll(c, (el) => el.getAttribute('type') === 'radio' && el.value === id)[0];
    const night = await themeCard(gameConfig(), (c) => change(radio(c, 'rubric'), {}));
    expect(night.saved).toMatchObject({
      theme: 'nord',
      day_theme: 'gruvbox',
      night_theme: 'rubric',
    });
    daylight.now = 'day';
    const day = await themeCard(gameConfig(), (c) => change(radio(c, 'rubric'), {}));
    expect(day.saved).toMatchObject({ day_theme: 'rubric', night_theme: 'obsidian-ember' });
    // The Night theme select fills its own slot, whatever shows.
    const picked = await themeCard(gameConfig(), (c) => {
      const [row] = findAll(c, (el) => el.getAttribute('data-st-anchor') === 'night-theme');
      const [found] = findAll(row, (el) => el.nodeName === 'SELECT');
      change(found, { target: { value: 'tokyo-night' } });
    });
    expect(picked.saved).toMatchObject({ day_theme: 'gruvbox', night_theme: 'tokyo-night' });
    daylight.now = null;
  });
});

describe('AccessibilityPage', () => {
  it('starts Blinking text off on Accessibility while your system reduces motion and keeps your choice', async () => {
    // The window above answers every media query, reduce motion among
    // them, as a match.
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    const blinking = async (ui: UiConfig): Promise<boolean> => {
      await act(async () => {
        root.render(
          createElement(AccessibilityPage, {
            target: { group: 'accessibility', section: 'motion', anchor: 'blink-text' },
            navSeq: 0,
            config: ui,
            setConfig: () => undefined,
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const [row] = findAll(container, (el) => el.getAttribute('data-st-anchor') === 'blink-text');
      const [toggle] = findAll(row, (el) => el.getAttribute('role') === 'switch');
      return (toggle as unknown as { checked: boolean }).checked;
    };
    expect(await blinking(config())).toBe(false);
    expect(await blinking({ ...config(), blink_text: true })).toBe(true);
    expect(await blinking({ ...config(), blink_text: false })).toBe(false);
    await act(async () => {
      root.unmount();
    });
  });

  it('draws Fit game colors after Color vision on Accessibility, on unless you turn it off', async () => {
    const fitSwitch = async (cfg: UiConfig) => {
      const container = doc.createElement('div');
      doc.body.appendChild(container);
      const root = createRoot(container as unknown as HTMLElement);
      await act(async () => {
        root.render(
          createElement(AccessibilityPage, {
            target: { group: 'accessibility' },
            navSeq: 0,
            config: cfg,
            setConfig: () => undefined,
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const anchors = findAll(container, (el) => el.getAttribute('data-st-anchor') !== null).map(
        (el) => el.getAttribute('data-st-anchor'),
      );
      const [row] = findAll(
        container,
        (el) => el.getAttribute('data-st-anchor') === 'fit-game-colors',
      );
      const [input] = findAll(row, (el) => el.getAttribute('role') === 'switch');
      const checked = (input as unknown as { checked: boolean }).checked;
      await act(async () => {
        root.unmount();
      });
      return { label: row.textContent, checked, anchors };
    };

    const on = await fitSwitch(config());
    expect(on.label).toContain('Fit game colors');
    expect(on.label).toContain('Settings keeps the theme as published');
    expect(on.checked).toBe(true);
    const at = on.anchors.indexOf('fit-game-colors');
    expect(on.anchors.slice(at - 1, at + 2)).toEqual([
      'color-vision',
      'fit-game-colors',
      'readable-highlights',
    ]);
    const off = await fitSwitch({ ...config(), fit_game_colors: false });
    expect(off.checked).toBe(false);
  });

  it('leads Color and contrast with Color vision, Typical until you pick another', async () => {
    const visionRow = async (cfg: UiConfig, pick?: string) => {
      const container = doc.createElement('div');
      doc.body.appendChild(container);
      const root = createRoot(container as unknown as HTMLElement);
      let saved: UiConfig | null = null;
      await act(async () => {
        root.render(
          createElement(AccessibilityPage, {
            target: { group: 'accessibility' },
            navSeq: 0,
            config: cfg,
            setConfig: (next) => {
              saved = next(cfg);
            },
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const anchors = findAll(container, (el) => el.getAttribute('data-st-anchor') !== null).map(
        (el) => el.getAttribute('data-st-anchor'),
      );
      const [row] = findAll(
        container,
        (el) => el.getAttribute('data-st-anchor') === 'color-vision',
      );
      const [select] = findAll(row, (el) => el.nodeName === 'SELECT');
      const options = select.options.map((o) => ({ label: o.textContent, value: o.value }));
      const value = select.options.find(
        (o) => (o as unknown as { selected?: boolean }).selected,
      )?.value;
      if (pick !== undefined) {
        // The fake DOM sends no events, so call the handler React keeps.
        const key = Object.keys(select).find((k) => k.startsWith('__reactProps$'));
        const props = key
          ? (select as unknown as Record<string, { onChange?: (e: unknown) => void }>)[key]
          : undefined;
        await act(async () => {
          props?.onChange?.({ target: { value: pick } });
        });
      }
      await act(async () => {
        root.unmount();
      });
      return { anchors, label: row.textContent, options, value, saved: saved as UiConfig | null };
    };

    const typical = await visionRow(config(), 'deuteranopia');
    // Screen reader sits above it (board 13).
    const color = typical.anchors.indexOf('color');
    expect(typical.anchors.slice(color, color + 3)).toEqual([
      'color',
      'color-vision',
      'fit-game-colors',
    ]);
    expect(typical.label).toContain('Color vision');
    expect(typical.label).toContain(
      'Vosh swaps the colors your eyes confuse for colors they tell apart, the way color blind modes in games do.',
    );
    // Typical changes nothing, so the row says nothing more.
    expect(typical.label).not.toContain('turn');
    expect(typical.options).toEqual([
      { label: 'Typical', value: 'typical' },
      { label: 'Deuteranopia', value: 'deuteranopia' },
      { label: 'Protanopia', value: 'protanopia' },
      { label: 'Tritanopia', value: 'tritanopia' },
    ]);
    expect(typical.value).toBe('typical');
    // A pick saves with the rest of the config.
    expect(typical.saved?.color_vision).toBe('deuteranopia');
    // The row says what the vision swaps, the same on every theme.
    const picked = await visionRow({ ...config(), color_vision: 'tritanopia' });
    expect(picked.value).toBe('tritanopia');
    expect(picked.label).toContain(
      'In the game text blues turn purple and magentas turn pink. The window keeps danger, warn, and success where you tell them apart, and makes them lighter or darker where they sit near. An accent Vosh picks moves clear of them.',
    );
    const kanso = { ...config(), theme: 'kanso-zen', color_vision: 'deuteranopia' as const };
    const swapped =
      'In the game text greens turn blue, reds lean toward orange, and blues toward violet, as far as your theme leaves room. In the window success turns blue and danger leans toward orange.';
    expect((await visionRow(kanso)).label).toContain(swapped);
    // Fit game colors off swaps the published colors, so the row says
    // the same.
    expect((await visionRow({ ...kanso, fit_game_colors: false })).label).toContain(swapped);
    // While the theme's colors are off for MUD text, the game text keeps
    // your base palette.
    const base = await visionRow({ ...kanso, theme_terminal_colors: false });
    expect(base.label).toContain(
      "Game text keeps your base palette while the theme's colors are off for MUD text. In the window success turns blue and danger leans toward orange.",
    );
  });

  it('draws Keep highlight colors readable under Color and contrast, on unless you turn it off', async () => {
    const readableSwitch = async (cfg: UiConfig) => {
      const container = doc.createElement('div');
      doc.body.appendChild(container);
      const root = createRoot(container as unknown as HTMLElement);
      await act(async () => {
        root.render(
          createElement(AccessibilityPage, {
            target: { group: 'accessibility' },
            navSeq: 0,
            config: cfg,
            setConfig: () => undefined,
            onError: () => undefined,
            pathB: false,
            navigate: () => undefined,
            setLeaveGuard: () => undefined,
          }),
        );
      });
      const [row] = findAll(
        container,
        (el) => el.getAttribute('data-st-anchor') === 'readable-highlights',
      );
      const [input] = findAll(row, (el) => el.getAttribute('role') === 'switch');
      const checked = (input as unknown as { checked: boolean }).checked;
      await act(async () => {
        root.unmount();
      });
      return { label: row.textContent, checked };
    };

    const on = await readableSwitch(config());
    expect(on.label).toContain('Keep highlight colors readable');
    expect(on.checked).toBe(true);
    const off = await readableSwitch({ ...config(), readable_highlights: false });
    expect(off.checked).toBe(false);
  });
});
