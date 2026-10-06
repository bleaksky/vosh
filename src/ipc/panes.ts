// The active profile's pane layout, which the backend saves and sends
// when it changes outside this window, and the reset of any profile's
// panes. panel/paneLayout.ts sanitizes every tree it reads. Each call
// returns the Tauri promise as it is. Then the panes a session's plugins
// draw with mud.pane, which stores/session/luaPanesStore.ts keeps.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PaneLayout } from '../panel/paneLayout';
import { LUA_PANES, PANE_LAYOUT_CHANGED } from './events';
import { sessionOf } from './session';

/** The active profile's pane layout, as the backend sends it. Pages
 *  read it through getPaneLayout in panel/paneLayout.ts, which cleans it. */
export function paneLayoutGet(): Promise<unknown> {
  return invoke<unknown>('pane_layout_get');
}

/** Save the active profile's pane layout, with the generation the
 *  edited tree was read at. False when the backend refused it, since the
 *  profile it came from has been swapped out since. Pages save through
 *  setPaneLayout in panel/paneLayout.ts, which batches a drag and keeps
 *  the save from coming back as a change. */
export function paneLayoutSet(save: {
  layout: PaneLayout;
  generation: number | null;
}): Promise<boolean> {
  return invoke<boolean>('pane_layout_set', { ...save });
}

/** Put a profile's panes back to the stock map over affects tree,
 *  keeping whether its panel shows and how wide it is. Returns the new
 *  layout as the backend sends it. The live profile saves it at once and
 *  every window hears it through vosh://pane-layout-changed. Pages reset
 *  through resetPaneLayout in panel/paneLayout.ts, which cleans the tree. */
export function paneLayoutReset(profile?: string | null): Promise<unknown> {
  return invoke<unknown>('pane_layout_reset', { profile: profile ?? null });
}

/** Hear a pane layout saved outside this window. Pages hear it through
 *  subscribePaneLayout in panel/paneLayout.ts, which cleans it and holds
 *  it back while a save of their own is out. */
export function subscribePaneLayoutChanged(cb: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>(PANE_LAYOUT_CHANGED, (event) => cb(event.payload));
}

/** One block of a Lua pane, `Block` in src-tauri/src/script/panes.rs.
 *  A line carries Vosh color codes like {red}. */
export type LuaBlock =
  | { kind: 'row'; label: string; value: string }
  | { kind: 'gauge'; label: string; value: number; max: number }
  | { kind: 'line'; text: string }
  | { kind: 'rule' };

/** A pane a plugin draws, by its plugin and the id mud.pane took, with
 *  its title, the words beside it, empty for none, and its blocks. */
export interface LuaPane {
  plugin: string;
  id: string;
  title: string;
  meta: string;
  blocks: LuaBlock[];
}

/** What changed in a session's Lua panes since the last send: each pane
 *  that changed, whole, and each one that went. */
export interface LuaPanesChange {
  panes: LuaPane[];
  removed: { plugin: string; id: string }[];
}

/** Every pane the plugins of `session` draw, by plugin and then id. */
export function luaPanesGet(session?: number): Promise<LuaPane[]> {
  return invoke<LuaPane[]>('lua_panes_get', { session });
}

/** Hear what the plugins of a session changed in their panes, once per
 *  flush, with that session. */
export function onLuaPanes(
  cb: (change: LuaPanesChange, session: number) => void,
): Promise<UnlistenFn> {
  return listen<LuaPanesChange & { session?: number }>(LUA_PANES, (event) => {
    const { panes, removed } = event.payload;
    cb({ panes, removed }, sessionOf(event.payload));
  });
}
