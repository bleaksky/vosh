import { act, createElement, useState } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { CustomTheme } from '../../../../ipc/theme';
import type { UiConfig } from '../../../../ipc/uiConfig';
import type { XtermPalette } from '../../../../theme/themes';
import { FakeDocument, findAll, type FakeNode } from '../../../../test/fakeDom';
import type { CustomThemeRows as CustomThemeRowsType } from './CustomThemeRows';

// A change to a color the game color fit reads drops the fit the theme
// kept and fits the new colors once they rest. The rows mount for real,
// into the stand in for the DOM in src/test/fakeDom.ts, and the fit,
// which runs in a worker, answers when the test says.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(undefined)) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

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
vi.mock('../../../../theme/fitOffThread', () => ({ fitOffThread: fitting.fitOffThread }));

const doc = new FakeDocument();
let CustomThemeRows: typeof CustomThemeRowsType;
let FIT_SETTLE_MS: number;
let createRoot: typeof import('react-dom/client').createRoot;
let normalizeUiConfig: typeof import('../../../../ipc/uiConfig').normalizeUiConfig;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    matchMedia: () => ({ matches: true, addEventListener() {}, removeEventListener() {} }),
  });
  vi.stubGlobal('navigator', { userAgent: 'node' });
  ({ createRoot } = await import('react-dom/client'));
  ({ CustomThemeRows, FIT_SETTLE_MS } = await import('./CustomThemeRows'));
  ({ normalizeUiConfig } = await import('../../../../ipc/uiConfig'));
});

afterAll(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const dusk: CustomTheme = {
  id: 'dusk',
  label: 'Dusk',
  description: '',
  xterm: { background: '#1a1b26', foreground: '#c0caf5' },
  chrome: {},
  fitted: { brightBlack: '#94989f', red: '#cb7b74' },
};

/** Run the color picker of the slot named `label` with `hex`. */
function pick(root: FakeNode, label: string, hex: string) {
  const [input] = findAll(
    root,
    (el) => el.getAttribute('aria-label') === `Choose the ${label} color`,
  );
  const key = Object.keys(input).find((k) => k.startsWith('__reactProps$')) ?? '';
  const props = (input as unknown as Record<string, { onChange: (e: unknown) => void }>)[key];
  props.onChange({ target: { value: hex } });
}

let shown: UiConfig;

function startConfig(): UiConfig {
  return normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: 'Menlo',
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
    custom_themes: [dusk],
  });
}

/** The rows on `shown`, with the config the Settings window keeps above
 *  them. Without `rows` the window shows another page. */
function Host({ rows = true }: { rows?: boolean }) {
  const [cfg, setCfg] = useState(shown);
  shown = cfg;
  if (!rows) return null;
  return createElement(CustomThemeRows, {
    config: cfg,
    update: (patch) =>
      setCfg((c) => {
        const change = typeof patch === 'function' ? patch(c) : patch;
        return change ? { ...c, ...change } : c;
      }),
  });
}

describe('CustomThemeRows', () => {
  it('drops the fit on an edit to a color it reads and fits the new colors once', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    shown = startConfig();
    await act(async () => {
      root.render(createElement(Host));
    });

    // A drag through the picker: two reds, then the one you keep.
    for (const hex of ['#ff0000', '#ee1111', '#dd2222']) {
      await act(async () => {
        pick(container, 'red', hex);
      });
    }
    expect(shown.custom_themes[0].xterm.red).toBe('#dd2222');
    expect(shown.custom_themes[0]).not.toHaveProperty('fitted');
    expect(fitting.asked).toHaveLength(0);

    await act(async () => {
      vi.advanceTimersByTime(FIT_SETTLE_MS);
    });
    expect(fitting.asked).toHaveLength(1);
    expect(fitting.asked[0].red).toBe('#dd2222');
    expect(fitting.asked[0].background).toBe('#1a1b26');

    await act(async () => {
      fitting.answer({ red: '#e0473f' });
    });
    expect(shown.custom_themes[0].fitted).toEqual({ red: '#e0473f' });

    // The cursor is not a color the fit reads, so the fit stays.
    await act(async () => {
      pick(container, 'cursor', '#ffffff');
      vi.advanceTimersByTime(FIT_SETTLE_MS);
    });
    expect(shown.custom_themes[0].fitted).toEqual({ red: '#e0473f' });
    expect(fitting.asked).toHaveLength(1);

    await act(async () => {
      root.unmount();
    });
  });

  it('fits colors still settling at once when you leave, and keeps the fit', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    fitting.asked.length = 0;
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    shown = startConfig();
    await act(async () => {
      root.render(createElement(Host));
    });
    await act(async () => {
      pick(container, 'red', '#cc3333');
    });
    expect(fitting.asked).toHaveLength(0);

    // You move to another page before the colors rest. The fit starts
    // then, and its answer lands on the config the window keeps.
    await act(async () => {
      root.render(createElement(Host, { rows: false }));
    });
    expect(fitting.asked).toHaveLength(1);
    expect(fitting.asked[0].red).toBe('#cc3333');
    await act(async () => {
      fitting.answer({ red: '#d94a44' });
    });
    expect(shown.custom_themes[0].fitted).toEqual({ red: '#d94a44' });

    await act(async () => {
      root.unmount();
    });
  });
});
