import { act, createElement, useState } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { UiConfig, UiFields } from '../../ipc/uiConfig';
import { FakeDocument, findAll, type FakeElement, type FakeNode } from '../../test/fakeDom';
import type {
  CustomizeVitalsSection as SectionType,
  VitalsList as VitalsListType,
} from './VitalsCustomize';

// Customize vitals under Settings, Layout, as boards 2 to 5 of the
// Vitals Styles review draw it. The section draws from the config it is
// handed. The list mounts for real into the stand in for the DOM in
// src/test/fakeDom.ts, so its grips take the keys a test presses.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(undefined)) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The theme and the panel layout come from the window's stores, which
// a test holds still: Kanso Zen and a 300 pt panel.
vi.mock('../../theme/useActiveTheme', async () => {
  const { findTheme } = await import('../../theme/themes');
  return { useActiveTheme: () => findTheme('kanso-zen') };
});
vi.mock('../../panel/panelLayoutStore', () => ({
  usePanelLayout: () => null,
  panelWidthOf: () => 300,
}));

const doc = new FakeDocument();
/** The window's keydown listeners, which the escape stack adds. */
const keyListeners: ((e: unknown) => void)[] = [];
let CustomizeVitalsSection: typeof SectionType;
let VitalsList: typeof VitalsListType;
let createRoot: typeof import('react-dom/client').createRoot;
let normalizeUiConfig: typeof import('../../ipc/uiConfig').normalizeUiConfig;
let VOSH_VITALS_TEXT: string;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener(type: string, listener: (e: unknown) => void) {
      if (type === 'keydown') keyListeners.push(listener);
    },
    removeEventListener() {},
    matchMedia: () => ({ matches: true, addEventListener() {}, removeEventListener() {} }),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: 'Linux x86_64' });
  vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => '' }));
  vi.stubGlobal(
    'MutationObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  ({ createRoot } = await import('react-dom/client'));
  ({ CustomizeVitalsSection, VitalsList } = await import('./VitalsCustomize'));
  ({ normalizeUiConfig } = await import('../../ipc/uiConfig'));
  ({ VOSH_VITALS_TEXT } = await import('../../ipc/vitals'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const config = (patch: Partial<UiConfig> = {}): UiConfig => ({
  ...normalizeUiConfig({
    theme: 'kanso-zen',
    auto_update: false,
    font_family: 'Menlo',
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  }),
  ...patch,
});

function draw(patch: Partial<UiConfig> = {}): string {
  return renderToStaticMarkup(
    <CustomizeVitalsSection config={config(patch)} update={() => undefined} />,
  );
}

/** The row labels, in order. */
const labels = (html: string) =>
  [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);

/** Each vital's row as `name on`, `name off`, with ` color` on a
 *  picked swatch and ` quiet` on one that rests. */
const vitals = (html: string) =>
  [...html.matchAll(/<li class="st-vital([^"]*)"[^>]*>.*?<\/li>/g)].map(([row, classes]) => {
    const name = /class="st-vital-name">([^<]*)</.exec(row)?.[1];
    const swatch = /<button[^>]*class="st-vital-swatch[^>]*>/.exec(row)?.[0] ?? '';
    const on = classes.includes('is-off') ? 'off' : 'on';
    const color = swatch.includes('is-default') ? '' : ' color';
    const quiet = swatch.includes('disabled=""') ? ' quiet' : '';
    return `${name} ${on}${color}${quiet}`;
  });

/** Each segment as `label pressed disabled`, in order. */
const segments = (html: string) =>
  [...html.matchAll(/<button type="button" class="st-seg-item"([^>]*)>(.*?)<\/button>/g)].map(
    ([, attrs, inner]) =>
      `${inner}${attrs.includes('aria-pressed="true"') ? ' pressed' : ''}${
        attrs.includes('disabled') ? ' disabled' : ''
      }`,
  );

/** Whether Reset to default rests. */
const resting = (html: string) => /<button[^>]*disabled=""[^>]*>Reset to default</.test(html);

describe('CustomizeVitalsSection', () => {
  it('draws the list, then your opponent, Values, Meter and the warning, as board 3 At rest', () => {
    const html = draw();
    expect(html).toContain('>Customize vitals</h2>');
    expect(html).toContain('Vitals and their order');
    expect(html).toContain('Drag a vital to move it. Turn one off to drop it from the panel.');
    expect(vitals(html)).toEqual(['Health on', 'Mana on', 'Moves on']);
    expect(html).toContain('aria-label="Move Health"');
    expect(html).toContain('aria-label="Show Moves"');
    expect(labels(html)).toEqual([
      'Vitals and their order',
      'Your opponent',
      'Values',
      'Meter',
      'Warn before you run low',
    ]);
    expect(html).toContain('In a fight, its name and its health in warn, in every style.');
    expect(html).toContain('aria-label="Show your opponent"');
    expect(segments(html)).toEqual([
      'On top pressed',
      'At the bottom',
      'Current and max pressed',
      'Current',
      'Percent',
      'Line pressed',
      'Bar',
      'None',
    ]);
    expect(html).toContain('Bar is easier to read in a fight. None keeps only the numbers.');
  });

  it('rests Reset to default until something differs', () => {
    expect(resting(draw())).toBe(true);
    expect(resting(draw({ vitals_colors: { mana: 12 } }))).toBe(false);
    expect(resting(draw({ vitals_order: ['move', 'hp', 'mana'] }))).toBe(false);
    expect(resting(draw({ vitals_off: ['opponent'] }))).toBe(false);
    expect(resting(draw({ vitals_warn_thirds: true }))).toBe(false);
    // Your style and where your vitals show sit above, so they never wake it.
    expect(resting(draw({ vitals_style: 'gauges', vitals_place: 'status' }))).toBe(true);
  });

  it('draws your order, the vitals you turned off and your colors, as board 3 Dropped', () => {
    const html = draw({
      vitals_order: ['move', 'hp', 'mana'],
      vitals_off: ['mana'],
      vitals_colors: { hp: 1 },
    });
    expect(vitals(html)).toEqual(['Moves on', 'Health on color', 'Mana off quiet']);
  });

  it('quiets Meter for the styles that draw their own mark', () => {
    const gauges = draw({ vitals_style: 'gauges' });
    expect(gauges).toContain('Gauges draw their own pill, so they take no meter.');
    expect(segments(gauges).slice(5)).toEqual([
      'Line pressed disabled',
      'Bar disabled',
      'None disabled',
    ]);
    expect(draw({ vitals_style: 'pips' })).toContain(
      'Pips draw their own discs, so they take no meter.',
    );
    expect(segments(draw({ vitals_style: 'ledger' })).slice(5)).toEqual([
      'Line pressed',
      'Bar',
      'None',
    ]);
  });

  it('quiets the swatches and Meter under Status line, as board 4 draws it', () => {
    const html = draw({ vitals_place: 'status', vitals_colors: { mana: 12 } });
    expect(html).toContain(
      'Drag a vital to move it. Turn one off to drop it from the status line. The status line keeps its quiet labels, so your colors wait for the panel.',
    );
    expect(vitals(html)).toEqual(['Health on quiet', 'Mana on color quiet', 'Moves on quiet']);
    expect(html).toContain('The status line draws no meter.');
    expect(segments(html).slice(5)).toEqual([
      'Line pressed disabled',
      'Bar disabled',
      'None disabled',
    ]);
  });

  it('holds your text and its preview under Text, as board 5 In Settings draws it', () => {
    const html = draw({ vitals_style: 'text', vitals_colors: { mana: 12 } });
    expect(labels(html)).toEqual(['Your vitals text']);
    expect(html).toContain(
      'Write your vitals with the codes your prompt uses. Your text decides which vitals show, their order, their colors and how values read.',
    );
    expect(html).toContain('Edit…');
    expect(html).not.toContain('Vitals and their order');
    expect(segments(html)).toEqual(['Now pressed', 'Low health', 'Fight']);
    // Vosh's text is the default there, whatever the shared set holds.
    expect(resting(html)).toBe(true);
    expect(resting(draw({ vitals_style: 'text', vitals_text: VOSH_VITALS_TEXT }))).toBe(true);
    expect(resting(draw({ vitals_style: 'text', vitals_text: '%hp %mana %move' }))).toBe(false);
  });
});

/** The React props of `el`. */
function props(el: FakeElement): Record<string, (e?: unknown) => void> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, Record<string, (e?: unknown) => void>>)[key];
}

/** Press `key` on the grip that moves `name`. */
function press(root: FakeNode, name: string, key: string) {
  const [grip] = findAll(root, (el) => el.getAttribute('aria-label') === `Move ${name}`);
  props(grip).onKeyDown({
    key,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    preventDefault() {},
  });
}

const names = (root: FakeNode) =>
  findAll(root, (el) => el.getAttribute('class') === 'st-vital-name').map((el) => el.textContent);

/** Mount the list on `start`, keeping each save as Settings does. */
async function mountList(start: UiConfig) {
  const saved: UiFields[] = [];
  function Host() {
    const [cfg, setCfg] = useState(start);
    return createElement(VitalsList, {
      config: cfg,
      quietColors: false,
      update: (patch: UiFields) => {
        saved.push(patch);
        setCfg((c) => ({ ...c, ...patch }));
      },
    });
  }
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(createElement(Host));
  });
  return { container, saved, unmount: () => act(() => root.unmount()) };
}

describe('moving a vital from the keyboard', () => {
  it('lifts Moves with Space, moves it with the arrow keys and drops it first', async () => {
    const { container, saved, unmount } = await mountList(config());
    await act(async () => press(container, 'Moves', ' '));
    expect(
      findAll(container, (el) => el.getAttribute('class') === 'st-vital is-lifted'),
    ).toHaveLength(1);
    await act(async () => press(container, 'Moves', 'ArrowUp'));
    await act(async () => press(container, 'Moves', 'ArrowUp'));
    await act(async () => press(container, 'Moves', 'ArrowUp'));
    expect(saved).toEqual([]);
    await act(async () => press(container, 'Moves', ' '));
    expect(saved).toEqual([{ vitals_order: ['move', 'hp', 'mana'] }]);
    expect(names(container)).toEqual(['Moves', 'Health', 'Mana']);
    expect(container.textContent).toContain('Moves, moved to 1 of 3');
    await unmount();
  });

  it('puts the vital back on Escape', async () => {
    const { container, saved, unmount } = await mountList(config());
    await act(async () => press(container, 'Health', ' '));
    await act(async () => press(container, 'Health', 'ArrowDown'));
    await act(async () => {
      for (const listener of keyListeners) {
        listener({ key: 'Escape', preventDefault() {}, stopPropagation() {}, target: null });
      }
    });
    expect(
      findAll(container, (el) => el.getAttribute('class') === 'st-vital is-lifted'),
    ).toHaveLength(0);
    // Space now lifts it again rather than dropping it a place down.
    await act(async () => press(container, 'Health', ' '));
    await act(async () => press(container, 'Health', ' '));
    expect(saved).toEqual([]);
    expect(names(container)).toEqual(['Health', 'Mana', 'Moves']);
    await unmount();
  });
});

describe('the color list', () => {
  it('names each swatch and its color, as a screen reader hears it', () => {
    const html = draw({ vitals_colors: { mana: 12 } });
    expect(html).toContain('aria-label="Color for Health, Default"');
    expect(html).toContain('aria-label="Color for Mana, Bright blue"');
  });
});
