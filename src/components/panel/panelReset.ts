import { invoke } from '@tauri-apps/api/core';
import { flushPaneLayout, layoutFromDock } from '../../lib/paneLayout';
import { pushToast } from '../../lib/toasts';
import { getPanelLayout, setPaneTree } from './panelLayoutStore';

// Reset panel layout, from the command palette. The active profile's
// panes go back to the tree its old dock layout migrates to, which is
// the stock map over affects for a profile that never arranged one.
// The panel keeps its width and whether it shows.

/** Replace the active profile's pane tree with its migration default,
 *  save it at once, and say so in a toast. */
export async function resetPanelLayout(): Promise<void> {
  try {
    if (getPanelLayout() === null) throw new Error('the pane layout has not loaded');
    const dock = await invoke<unknown>('dock_layout_get');
    setPaneTree(layoutFromDock(dock).root);
    await flushPaneLayout();
    pushToast({ kind: 'success', message: 'Panel layout reset' });
  } catch (e) {
    console.error('[panel] reset failed', e);
    pushToast({ kind: 'error', message: 'Vosh could not reset the panel layout' });
  }
}
