import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultLayout, layoutFromDock, type PaneLayout } from '../../lib/paneLayout';

// The palette's Reset panel layout row and what it runs. The store and
// the pane layout module are real, the backend and toasts are fakes.
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
  generation: 7,
};

const OLD_DOCK = [
  { id: 'map', zone: 'right', align: 'top' },
  { id: 'group', zone: 'right', align: 'top' },
  { id: 'affects', zone: 'right', align: 'bottom' },
];

let dock: unknown = OLD_DOCK;
let failWrite = false;

tauri.invoke.mockImplementation((cmd: string) => {
  if (cmd === 'pane_layout_get') return Promise.resolve(ARRANGED);
  if (cmd === 'dock_layout_get') return Promise.resolve(dock);
  if (cmd === 'pane_layout_set') {
    return failWrite ? Promise.reject(new Error('disk full')) : Promise.resolve(true);
  }
  return Promise.resolve();
});

const writes = () =>
  tauri.invoke.mock.calls
    .filter((c) => c[0] === 'pane_layout_set')
    .map((c) => c[1] as { layout: PaneLayout; generation: number | null });

startPanelLayoutStore();
await vi.waitFor(() => expect(getPanelLayout()).not.toBeNull());

beforeEach(() => {
  tauri.invoke.mockClear();
  tauri.pushToast.mockClear();
  dock = OLD_DOCK;
  failWrite = false;
  setPaneTree(ARRANGED.root);
});

describe('resetPanelLayout', () => {
  it('puts back the tree the old dock layout migrates to and saves it at once', async () => {
    await resetPanelLayout();
    const expected = layoutFromDock(OLD_DOCK).root;
    expect(expected.children.map((c) => c.id)).toEqual(['map', 'group', 'affects']);
    expect(getPanelLayout()?.root).toEqual(expected);
    // One write, not a debounced one, made against the profile it read.
    expect(writes()).toHaveLength(1);
    expect(writes()[0].layout.root).toEqual(expected);
    expect(writes()[0].generation).toBe(7);
    expect(tauri.pushToast).toHaveBeenCalledWith({
      kind: 'success',
      message: 'Panel layout reset',
    });
  });

  it('gives map over affects to a profile with no old dock layout', async () => {
    dock = [];
    await resetPanelLayout();
    expect(getPanelLayout()?.root).toEqual(defaultLayout().root);
    expect(writes()[0].layout.root).toEqual(defaultLayout().root);
  });

  it('keeps the panel width and whether the panel shows', async () => {
    await resetPanelLayout();
    expect(getPanelLayout()).toMatchObject({ panel_open: false, panel_width: 360 });
    expect(writes()[0].layout).toMatchObject({ panel_open: false, panel_width: 360 });
  });

  it('says so when the save fails', async () => {
    failWrite = true;
    const log = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    await resetPanelLayout();
    log.mockRestore();
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
  });
});
