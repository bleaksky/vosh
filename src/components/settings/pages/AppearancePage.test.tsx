import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { SystemFontEntry, UiConfig } from '../../../lib/session';
import { FakeDocument, findAll, type FakeNode } from '../../../test/fakeDom';
import type { AppearancePage as AppearancePageType } from './AppearancePage';

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
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// ── The page ─────────────────────────────────────────────────────────

const doc = new FakeDocument();
let AppearancePage: typeof AppearancePageType;
let createRoot: typeof import('react-dom/client').createRoot;
let normalizeUiConfig: typeof import('../../../lib/session').normalizeUiConfig;
let BUILTIN_THEMES: typeof import('../../../lib/themes').BUILTIN_THEMES;

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
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  // React DOM reads navigator.userAgent when it loads. Node 20, the CI
  // version, has no navigator of its own.
  vi.stubGlobal('navigator', { userAgent: 'node' });
  // React DOM checks for a DOM once, when it loads, so it loads now.
  ({ createRoot } = await import('react-dom/client'));
  ({ AppearancePage } = await import('./AppearancePage'));
  ({ normalizeUiConfig } = await import('../../../lib/session'));
  ({ BUILTIN_THEMES } = await import('../../../lib/themes'));
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
      { label: 'Berkeley Mono', value: '"BerkeleyMono Bundled", Menlo, monospace' },
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
      { label: 'Berkeley Mono', value: '"BerkeleyMono Bundled", Menlo, monospace' },
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
});
