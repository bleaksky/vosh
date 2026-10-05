import { acceptPaneLayout, flushPaneLayout, resetPaneLayout } from '../../lib/paneLayout';
import { pushToast } from '../../stores/toasts';
import { getPanelLayout } from './panelLayoutStore';

// Reset panel layout, from the command palette. The active profile's
// panes go back to the stock map over affects tree through the same
// backend command as Reset to default in Settings > Characters, so both
// resets agree. The panel keeps its width and whether it shows.

/** Put the active profile's stock tree back, save it at once, show it,
 *  and say so in a toast. */
export async function resetPanelLayout(): Promise<void> {
  try {
    if (getPanelLayout() === null) throw new Error('the pane layout has not loaded');
    // Send a drag still waiting on its debounce first, so the reset
    // lands after it and shows at once. A write that fails here is
    // dropped, and the reset replaces it anyway.
    await flushPaneLayout().catch((e: unknown) => console.warn('[panel] pending write failed', e));
    acceptPaneLayout(await resetPaneLayout());
    pushToast({ kind: 'success', message: 'Panel layout reset' });
  } catch (e) {
    console.error('[panel] reset failed', e);
    pushToast({ kind: 'error', message: 'Vosh could not reset the panel layout' });
  }
}
