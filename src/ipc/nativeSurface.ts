// The native surface that draws the terminal under the page: the calls
// the page makes into it and the events it sends back. Each call returns
// the invoke promise and each subscribe the listen promise as they are,
// so every caller keeps its own catch and its own guards. A command with
// one key takes its value. One with more takes an object of its keys and
// spreads it into an object literal, where the contract test reads them.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  NATIVE_COPIED,
  NATIVE_GRID_SIZE,
  NATIVE_SCROLL,
  TERMINAL_CLICKED,
  TERMINAL_CURSOR,
} from './events';

/** Where the pane sits in the window, in CSS px, with the device pixel
 *  ratio and the rows at its bottom the pinned prompt band borrows. */
export function nativeSurfaceSetBounds(bounds: {
  x: number;
  y: number;
  width: number;
  height: number;
  dpr: number;
  lent: number;
}): Promise<void> {
  return invoke('native_surface_set_bounds', { ...bounds });
}

/** A pointer event over the pane, in pane-local CSS px. */
export function nativeSurfacePointer(pointer: {
  kind: string;
  x: number;
  y: number;
  open: boolean;
}): Promise<void> {
  return invoke('native_surface_pointer', { ...pointer });
}

/** True once the surface installed and its GPU came up. */
export function nativeSurfaceReady(): Promise<boolean> {
  return invoke('native_surface_ready');
}

/** A wheel delta in AppKit's sign. Positive reveals older lines. */
export function nativeSurfaceWheel(deltaY: number): Promise<void> {
  return invoke('native_surface_wheel', { deltaY });
}

/** Copy the selection in a session's grid to the clipboard, the
 *  selected session's when it names none. */
export function nativeSurfaceCopy(session?: number): Promise<void> {
  return invoke('native_surface_copy', { session });
}

/** Select everything in a session's grid, scrollback included, the
 *  selected session's when it names none. */
export function nativeSurfaceSelectAll(session?: number): Promise<void> {
  return invoke('native_surface_select_all', { session });
}

/** The ground, text and selection colors and the 16 ANSI colors. */
export function nativeSurfaceSetTheme(theme: {
  background: string;
  foreground: string;
  selection: string;
  ansi: string[];
}): Promise<void> {
  return invoke('native_surface_set_theme', { ...theme });
}

/** The split divider's color setting. Null restores the default. */
export function nativeSurfaceSetDividerColor(color: string | null): Promise<void> {
  return invoke('native_surface_set_divider_color', { color });
}

/** The chrome colors the page derives from its theme tokens. A null
 *  color falls back to one derived from the terminal palette. */
export function nativeSurfaceSetTokens(tokens: {
  divider: string;
  selection: string;
  selectionText: string;
  findMatch: string | null;
  currentMatch: string | null;
  link: string;
  scrollbar: string;
  selrow: string;
  appearance: string;
}): Promise<void> {
  return invoke('native_surface_set_tokens', { ...tokens });
}

/** Whether a session's grid draws a band under each lifted prompt, the
 *  selected session's when it names none. */
export function nativeSurfaceSetPromptBands(on: boolean, session?: number): Promise<void> {
  return invoke('native_surface_set_prompt_bands', { on, session });
}

/** How far the band under the open row reaches past its last glyph, in
 *  CSS px, for the prompt card's line break mark and caret. */
export function nativeSurfaceSetPromptReach(px: number): Promise<void> {
  return invoke('native_surface_set_prompt_reach', { px });
}

/** Whether bright ANSI colors draw in the bold weight. */
export function nativeSurfaceSetBrightBold(on: boolean): Promise<void> {
  return invoke('native_surface_set_bright_bold', { on });
}

/** Whether blinking text blinks or draws steady. */
export function nativeSurfaceSetBlinkText(on: boolean): Promise<void> {
  return invoke('native_surface_set_blink_text', { on });
}

/** xterm's device cell and glyph box, so the grid spaces its cells and
 *  sets its baseline where xterm does. */
export function nativeSurfaceSetCellMetrics(metrics: {
  width: number;
  height: number;
  charHeight: number | null;
}): Promise<void> {
  return invoke('native_surface_set_cell_metrics', { ...metrics });
}

/** Search a session's grid and step to the next or previous match, the
 *  selected session's when it names none. Resolves to `[current, total]`,
 *  counted from 1, and `[0, 0]` with no match. */
export function nativeSurfaceFind(
  search: {
    query: string;
    regex: boolean;
    caseSensitive: boolean;
    wholeWord: boolean;
    forward: boolean;
  },
  session?: number,
): Promise<[number, number]> {
  return invoke('native_surface_find', { ...search, session });
}

/** Clear the search highlight in a session's grid, the selected
 *  session's when it names none. */
export function nativeSurfaceFindClear(session?: number): Promise<void> {
  return invoke('native_surface_find_clear', { session });
}

/** The CSS font list xterm draws with and its size, for the grid's atlas. */
export function nativeSurfaceSetFont(font: { family: string; size: number }): Promise<void> {
  return invoke('native_surface_set_font', { ...font });
}

/** Scroll a session's grid from the keyboard, the selected session's
 *  when it names none. Toggle opens or closes the split the way a middle
 *  click does. */
export function nativeSurfaceScroll(
  kind: 'pageup' | 'pagedown' | 'bottom' | 'toggle',
  session?: number,
): Promise<void> {
  return invoke('native_surface_scroll', { kind, session });
}

/** The grid's size changed, as `[cols, rows]`. */
export function onNativeGridSize(cb: (size: [number, number]) => void): Promise<UnlistenFn> {
  return listen<[number, number]>(NATIVE_GRID_SIZE, (event) => cb(event.payload));
}

/** How far back the grid shows changed, as `[offset, max]` in rows. */
export function onNativeScroll(cb: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>(NATIVE_SCROLL, (event) => cb(event.payload));
}

/** The grid copied your selection. The payload is the count of
 *  characters copied. */
export function onNativeCopied(cb: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>(NATIVE_COPIED, (event) => cb(event.payload));
}

/** A click the page forwarded to the grid ended. */
export function onTerminalClicked(cb: () => void): Promise<UnlistenFn> {
  return listen(TERMINAL_CLICKED, () => cb());
}

/** The pointer over the grid wants another cursor, by its CSS name. */
export function onTerminalCursor(cb: (cursor: string) => void): Promise<UnlistenFn> {
  return listen<string>(TERMINAL_CURSOR, (event) => cb(event.payload));
}
