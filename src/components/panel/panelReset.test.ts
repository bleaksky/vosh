import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultLayout, type PaneLayout, type PaneSplit } from '../../lib/paneLayout';

// The palette's Reset panel layout row and what it runs. The store and
// the pane layout module are real. The backend is a fake that keeps one
// saved tree and the pane generation, refuses a write made against an
// older generation, and resets the way pane_layout_reset does.
const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  pushToast: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: tauri.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../../lib/toasts', () => ({ pushToast: tauri.pushToast }));

const { buildPaletteEntries, initialSelection, paletteSections } =
  await import('../../lib/palette');
const { getPanelLayout, setPaneTree, startPanelLayoutStore } = await import('./panelLayoutStore');
const { flushPaneLayout } = await import('../../lib/paneLayout');
const { resetPanelLayout } = await import('./panelReset');

// A profile that arranged its own panes: group over chat, a wider
// panel, hidden.
const ARRANGED: PaneLayout = {
  version: 1,
  panel_open: false,
  panel_width: 360,
  root: {
    id: 'root',
    split: 'column',
    weight: 1,
    children: [
      { id: 'group', pane: 'group', weight: 0.5, props: {} },
      { id: 'chat', pane: 'chat', weight: 0.5, props: { channel: 'tell' } },
    ],
  },
};

// The same panes side by side, as a splitter drag might leave them.
const DRAGGED: PaneSplit = { ...ARRANGED.root, split: 'row' };

let saved: PaneLayout = ARRANGED;
let generation = 7;
let failReset = false;

tauri.invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
  if (cmd === 'pane_layout_get') return Promise.resolve({ ...saved, generation });
  if (cmd === 'pane_layout_set') {
    if (args?.generation !== null && args?.generation !== generation) {
      return Promise.resolve(false);
    }
    saved = args?.layout as PaneLayout;
    return Promise.resolve(true);
  }
  if (cmd === 'pane_layout_reset') {
    if (failReset) return Promise.reject(new Error('disk full'));
    generation += 1;
    saved = { ...saved, root: defaultLayout().root };
    return Promise.resolve({ ...saved, generation });
  }
  return Promise.resolve();
});

const calls = (cmd: string) => tauri.invoke.mock.calls.filter((c) => c[0] === cmd);

startPanelLayoutStore();
await vi.waitFor(() => expect(getPanelLayout()).not.toBeNull());

beforeEach(async () => {
  failReset = false;
  setPaneTree(ARRANGED.root);
  await flushPaneLayout();
  tauri.invoke.mockClear();
  tauri.pushToast.mockClear();
});

describe('resetPanelLayout', () => {
  it('puts back the stock map over affects tree through the backend', async () => {
    const before = generation;
    await resetPanelLayout();
    expect(calls('pane_layout_reset')).toEqual([['pane_layout_reset', { profile: null }]]);
    expect(getPanelLayout()?.root).toEqual(defaultLayout().root);
    // The backend saves the reset, so this window writes nothing.
    expect(calls('pane_layout_set')).toHaveLength(0);
    expect(calls('dock_layout_get')).toHaveLength(0);
    // The next edit targets the tree the reset made.
    expect(getPanelLayout()?.generation).toBe(before + 1);
    expect(tauri.pushToast).toHaveBeenCalledWith({
      kind: 'success',
      message: 'Panel layout reset',
    });
  });

  it('keeps the panel width and whether the panel shows', async () => {
    await resetPanelLayout();
    expect(getPanelLayout()).toMatchObject({ panel_open: false, panel_width: 360 });
  });

  it('sends a drag still waiting to save before the reset, so the reset wins', async () => {
    setPaneTree(DRAGGED);
    await resetPanelLayout();
    const order = tauri.invoke.mock.calls
      .map((c) => c[0] as string)
      .filter((cmd) => cmd === 'pane_layout_set' || cmd === 'pane_layout_reset');
    expect(order).toEqual(['pane_layout_set', 'pane_layout_reset']);
    expect(getPanelLayout()?.root).toEqual(defaultLayout().root);
    expect(saved.root).toEqual(defaultLayout().root);
  });

  it('says so when the reset fails', async () => {
    failReset = true;
    const log = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    await resetPanelLayout();
    log.mockRestore();
    expect(getPanelLayout()?.root).toEqual(ARRANGED.root);
    expect(tauri.pushToast).toHaveBeenCalledWith({
      kind: 'error',
      message: 'Vosh could not reset the panel layout',
    });
    expect(tauri.pushToast).not.toHaveBeenCalledWith(
      expect.objectContaining({ message: 'Panel layout reset' }),
    );
  });
});

describe('Reset panel layout in the palette', () => {
  const entries = () =>
    buildPaletteEntries({
      connected: true,
      paneTypes: ['map', 'affects'],
      paneVisible: () => true,
      togglePane: () => {},
      openHelp: () => {},
      openFind: () => {},
      openSettingsTab: () => {},
      connect: () => {},
      disconnect: () => {},
      insertInput: () => {},
    });

  it('sits in View and runs the reset', () => {
    const row = entries().find((e) => e.title === 'Reset panel layout');
    expect(row).toMatchObject({ section: 'view', destructive: true, searchOnly: true });
    expect(row?.run).toBe(resetPanelLayout);
  });

  it('never becomes the row the palette opens on', () => {
    const all = entries();
    const home = paletteSections(all, '', ['panel-reset']);
    expect(home.map((s) => s.label)).toEqual(['View', 'Session']);
    const homeRows = home.flatMap((s) => s.rows);
    expect(homeRows.some((r) => r.id === 'panel-reset')).toBe(false);
    expect(homeRows[initialSelection(homeRows)].destructive).toBeFalsy();
    const found = paletteSections(all, 'reset panel', []).flatMap((s) => s.rows);
    expect(found.map((r) => r.id)).toEqual(['panel-reset']);
    expect(initialSelection(found)).toBe(-1);
    expect(initialSelection(found, 'reset panel')).toBe(0);
  });
});
