import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UiConfig, UiFields } from '../../ipc/uiConfig';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import type { AccessibilityPage as AccessibilityPageType } from './AccessibilityPage';

// The Screen reader section on top of Accessibility (board 13). Each of
// its four rows saves its own field at once, through the page's one
// writer, which this test stands in for.

const saves = vi.hoisted(() => [] as { patch: UiFields; now: boolean | undefined }[]);
vi.mock('../useSettingsAutoSave', () => ({
  useSettingsAutoSave: () => ({
    update: (patch: UiFields, options?: { now?: boolean }) =>
      saves.push({ patch, now: options?.now }),
  }),
}));

const doc = new FakeDocument();
let AccessibilityPage: typeof AccessibilityPageType;
let createRoot: typeof import('react-dom/client').createRoot;
let normalizeUiConfig: typeof import('../../ipc/uiConfig').normalizeUiConfig;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  });
  vi.stubGlobal('navigator', { userAgent: 'Mac' });
  ({ createRoot } = await import('react-dom/client'));
  ({ AccessibilityPage } = await import('./AccessibilityPage'));
  ({ normalizeUiConfig } = await import('../../ipc/uiConfig'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  saves.length = 0;
});

/** Mount the page on `ui`, run `act` on its container, and unmount. */
async function onPage(ui: Partial<UiConfig>, run: (root: FakeElement) => void) {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(
      createElement(AccessibilityPage, {
        target: { group: 'accessibility' },
        navSeq: 0,
        config: {
          ...normalizeUiConfig({
            theme: 'nord',
            auto_update: false,
            font_family: 'Menlo, monospace',
            font_size: 14,
            tracked_affects: [],
            enabled_presets: [],
          }),
          ...ui,
        },
        setConfig: () => undefined,
        onError: () => undefined,
        pathB: false,
        navigate: () => undefined,
        setLeaveGuard: () => undefined,
      }),
    );
  });
  await act(async () => run(container));
  await act(async () => root.unmount());
}

const row = (root: FakeElement, anchor: string) =>
  findAll(root, (el) => el.getAttribute('data-st-anchor') === anchor)[0];

const control = (root: FakeElement, anchor: string) =>
  findAll(row(root, anchor), (el) => el.nodeName === 'INPUT' || el.nodeName === 'SELECT')[0];

/** Call the onChange React keeps on an element. The fake DOM sends no
 *  events. */
function change(el: FakeElement, event: unknown) {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  (el as unknown as Record<string, { onChange: (e: unknown) => void }>)[key].onChange(event);
}

describe('AccessibilityPage, Screen reader', () => {
  it('draws the section first, its four rows in the order the board draws them', async () => {
    await onPage({}, (root) => {
      const anchors = findAll(root, (el) => el.getAttribute('data-st-anchor') !== null).map((el) =>
        el.getAttribute('data-st-anchor'),
      );
      expect(anchors.slice(0, 6)).toEqual([
        'screen-reader',
        'read-game-lines',
        'read-in-background',
        'read-your-prompt',
        'long-bursts',
        'color',
      ]);
      expect(row(root, 'screen-reader').textContent).toContain('Screen reader');
    });
  });

  it('saves each toggle to its own field at once', async () => {
    const toggles = {
      'read-game-lines': 'screen_reader',
      'read-in-background': 'screen_reader_background',
      'read-your-prompt': 'screen_reader_prompt',
    } as const;
    await onPage({}, (root) => {
      for (const anchor of Object.keys(toggles) as (keyof typeof toggles)[]) {
        const input = control(root, anchor);
        expect(input.getAttribute('role')).toBe('switch');
        expect((input as unknown as { checked: boolean }).checked).toBe(false);
        change(input, { target: { checked: true } });
      }
    });
    expect(saves).toEqual(
      Object.values(toggles).map((field) => ({ patch: { [field]: true }, now: true })),
    );
  });

  it('shows each switch as the config holds it, and keeps the rows live while the reader is off', async () => {
    await onPage(
      { screen_reader: false, screen_reader_background: true, screen_reader_prompt: true },
      (root) => {
        const on = (anchor: string) =>
          (control(root, anchor) as unknown as { checked: boolean }).checked;
        expect([on('read-game-lines'), on('read-in-background'), on('read-your-prompt')]).toEqual([
          false,
          true,
          true,
        ]);
        for (const anchor of ['read-in-background', 'read-your-prompt', 'long-bursts'])
          expect(control(root, anchor).getAttribute('disabled')).toBeNull();
      },
    );
  });

  it('offers bursts of 4, 8, 16 and 32 lines, starts on 8, and saves the pick as a number', async () => {
    await onPage({}, (root) => {
      const select = control(root, 'long-bursts');
      expect(select.options.map((o) => [o.value, o.textContent])).toEqual([
        ['4', '4 lines'],
        ['8', '8 lines'],
        ['16', '16 lines'],
        ['32', '32 lines'],
      ]);
      expect(
        select.options.find((o) => (o as unknown as { selected?: boolean }).selected)?.value,
      ).toBe('8');
      change(select, { target: { value: '16' } });
    });
    expect(saves).toEqual([{ patch: { screen_reader_burst: 16 }, now: true }]);
  });

  it('shows the key that reads your prompt at the end of its row', async () => {
    await onPage({}, (root) => {
      const caps = findAll(row(root, 'read-your-prompt'), (el) => el.nodeName === 'KBD').map(
        (el) => el.textContent,
      );
      expect(caps).toEqual(['⇧', '⌘', 'P']);
      expect(row(root, 'read-your-prompt').textContent).toContain(
        'Your prompt comes every pulse. Off reads it only when you press',
      );
    });
  });
});
