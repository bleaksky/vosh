import { profileDisplayName } from '../../lib/characterProfiles';
import { profileInFront } from '../../stores/session/sessionsStore';
import { pushToast } from '../../stores/toasts';
import {
  acceptPaneLayout,
  defaultLayout,
  flushPaneLayout,
  sanitizeLayout,
  setProfileTrees,
} from '../paneLayout';
import { loneChatToAll } from '../paneActions';
import { getPanelLayout, setPanelOpen } from '../panelLayoutStore';
import { arrangeFrom, getArranging, startArranging, stopArranging } from './arrange';

// Turning arrange mode on and off. Done saves the draft for the profile
// in front through pane_trees_set, which keeps the layout it replaces
// among the earlier ones, and says so. The window shows the draft at
// once rather than waiting on the save. Cancel drops the draft.

/** Turn arrange mode on for the profile in front. A pane resize still
 *  waiting on its debounce saves first, so the draft starts from it. */
export async function enterArrangeMode(): Promise<void> {
  if (getArranging()) return;
  await flushPaneLayout().catch((e: unknown) => console.warn('[arrange] pending write failed', e));
  const layout = getPanelLayout();
  const profile = profileInFront();
  if (!layout || !profile) return;
  startArranging(layout, profile);
}

/** Done: save the draft when it changed, and show the panel after if it
 *  was hidden and now holds panes. */
export async function finishArranging(): Promise<void> {
  const a = stopArranging();
  if (!a) return;
  const changed =
    JSON.stringify([a.draft.root, a.draft.sidebar]) !==
    JSON.stringify([a.entered.root, a.entered.sidebar]);
  if (!changed) return;
  const owner = profileDisplayName(a.profile);
  // A Chat pane you left alone shows All, as closing the others does.
  const next = loneChatToAll(a.entered, {
    ...a.entered,
    root: a.draft.root,
    sidebar: a.draft.sidebar,
  });
  acceptPaneLayout(next);
  try {
    acceptPaneLayout(await setProfileTrees(a.profile, next, true));
    if (!a.entered.panel_open && next.root.children.length > 0) setPanelOpen(true);
    pushToast({ kind: 'success', message: `Saved the layout for ${owner}` });
  } catch (e) {
    console.error('[arrange] save failed', e);
    acceptPaneLayout(a.entered);
    pushToast({ kind: 'error', message: `Vosh could not save the layout for ${owner}` });
  }
}

/** Cancel: put back the layout arrange mode started from. */
export function cancelArranging(): void {
  stopArranging();
}

/** Start the draft again from the stock layout, as one step. */
export function arrangeDefault(): void {
  arrangeFrom(defaultLayout());
}

/** Start the draft again from an earlier layout, as one step. */
export function arrangeEarlier(panes: unknown): void {
  arrangeFrom(sanitizeLayout(panes));
}
