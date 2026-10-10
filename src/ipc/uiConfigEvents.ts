// The events that carry one UI config field or one group of fields,
// each heard on its own, so a window follows a save made in another.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toColorVision, type ColorVision } from '../theme/gameFit';
import {
  BASE_ANSI_CHANGED,
  BLINK_TEXT_CHANGED,
  BRIGHT_BOLD_CHANGED,
  CHIP_STYLE_CHANGED,
  COLOR_VISION_CHANGED,
  ECHO_MACROS_CHANGED,
  FIT_GAME_COLORS_CHANGED,
  FONT_CHANGED,
  GAME_TIME_CHANGED,
  INPUT_CURSOR_STYLE_CHANGED,
  INPUT_ECHO_COLOR_CHANGED,
  INPUT_LINE_MARK_CHANGED,
  KEEP_LAST_CHANGED,
  PASTE_LINE_DELAY_CHANGED,
  READABLE_HIGHLIGHTS_CHANGED,
  SCROLLBACK_LINES_CHANGED,
  SPELLCHECK_PROMPT_CHANGED,
  WRITING_OFFER_CHANGED,
  WRITING_ASK_POST_CHANGED,
  SPLIT_DIVIDER_CHANGED,
  STATUS_STYLE_CHANGED,
  PANEL_SIDE_CHANGED,
  TERMINAL_LINE_HEIGHT_CHANGED,
  THEME_TERMINAL_COLORS_CHANGED,
  TICK_COUNT_CHANGED,
} from './events';
import {
  normalizeChipStyle,
  normalizeGameTime,
  normalizeScrollbackLines,
  normalizeStatusStyle,
  normalizePanelSide,
  normalizeTerminalLineHeight,
  normalizeTickCount,
  type ChipStyle,
  type GameTime,
  type StatusStyle,
  type PanelSide,
  type TerminalLineHeight,
  type TickCount,
} from './uiConfig';

/** What FONT_CHANGED carries. */
export interface FontChange {
  family: string;
  size: number;
  /** The Panel font as saved (panelFont.ts). */
  panel: string;
  /** The panel size as saved, 0 for the terminal size (panelSize.ts). */
  panelSize: number;
}

/** Hear a new chip style saved from Settings. The Settings save emits
 *  it to every window, so the main window's status line follows at
 *  once. */
export async function subscribeChipStyleChanged(
  cb: (value: ChipStyle) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CHIP_STYLE_CHANGED, (event) => {
    cb(normalizeChipStyle(event.payload));
  });
}

/** Hear a new tick count saved from Settings. The Settings save emits
 *  it to every window, so the main window's status line follows at
 *  once. */
export async function subscribeTickCountChanged(
  cb: (value: TickCount) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TICK_COUNT_CHANGED, (event) => {
    cb(normalizeTickCount(event.payload));
  });
}

/** Hear a new game time clock saved from Settings, or the one a
 *  profile switch brings. The Settings save emits it to every window,
 *  so the main window's status line follows at once. */
export async function subscribeGameTimeChanged(cb: (value: GameTime) => void): Promise<UnlistenFn> {
  return listen<unknown>(GAME_TIME_CHANGED, (event) => {
    cb(normalizeGameTime(event.payload));
  });
}

/** Hear a new status bar style saved from Settings, or the one a
 *  profile switch brings. */
export async function subscribeStatusStyleChanged(
  cb: (value: StatusStyle) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(STATUS_STYLE_CHANGED, (event) => {
    cb(normalizeStatusStyle(event.payload));
  });
}

/** Hear a new panel side saved from Settings, or the one a profile
 *  switch brings. */
export async function subscribePanelSideChanged(
  cb: (value: PanelSide) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PANEL_SIDE_CHANGED, (event) => {
    cb(normalizePanelSide(event.payload));
  });
}

/** Hear a new terminal line height saved from Settings. */
export async function subscribeTerminalLineHeightChanged(
  cb: (value: TerminalLineHeight) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TERMINAL_LINE_HEIGHT_CHANGED, (event) => {
    cb(normalizeTerminalLineHeight(event.payload));
  });
}

/** Hear Scrollback size change. */
export async function subscribeScrollbackLinesChanged(
  cb: (lines: number) => void,
): Promise<UnlistenFn> {
  return listen<number>(SCROLLBACK_LINES_CHANGED, (event) => {
    cb(normalizeScrollbackLines(event.payload));
  });
}

/** Hear the Blinking text choice change, null for none. */
export async function subscribeBlinkTextChanged(
  cb: (value: boolean | null) => void,
): Promise<UnlistenFn> {
  return listen<boolean | null>(BLINK_TEXT_CHANGED, (event) => {
    cb(typeof event.payload === 'boolean' ? event.payload : null);
  });
}

export async function subscribeBrightBoldChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(BRIGHT_BOLD_CHANGED, (event) => {
    cb(Boolean(event.payload));
  });
}

/** Hear Fit game colors change, saved in Settings or brought by
 *  another profile. */
export async function subscribeFitGameColorsChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(FIT_GAME_COLORS_CHANGED, (event) => {
    cb(event.payload !== false);
  });
}

/** Hear the color vision change, saved in Settings or brought by
 *  another profile. */
export async function subscribeColorVisionChanged(
  cb: (value: ColorVision) => void,
): Promise<UnlistenFn> {
  return listen<string>(COLOR_VISION_CHANGED, (event) => {
    cb(toColorVision(event.payload));
  });
}

/** Hear Keep highlight colors readable change, saved in Settings or
 *  brought by another profile. */
export async function subscribeReadableHighlightsChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(READABLE_HIGHLIGHTS_CHANGED, (event) => {
    cb(event.payload !== false);
  });
}

export async function subscribeSplitDividerChanged(
  cb: (color: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>(SPLIT_DIVIDER_CHANGED, (event) => {
    cb(typeof event.payload === 'string' && event.payload.length > 0 ? event.payload : null);
  });
}

export async function subscribeBaseAnsiChanged(
  cb: (colors: string[] | null) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(BASE_ANSI_CHANGED, (event) => {
    const p = event.payload;
    cb(
      Array.isArray(p) && p.length === 16 && p.every((c) => typeof c === 'string')
        ? (p as string[])
        : null,
    );
  });
}

/** Hear the terminal font and size and the panel font and size saved in
 *  Settings. */
export function subscribeFontChanged(cb: (change: FontChange) => void): Promise<UnlistenFn> {
  return listen<FontChange>(FONT_CHANGED, (event) => cb(event.payload));
}

/** Hear Keep last command change. */
export function subscribeKeepLastChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(KEEP_LAST_CHANGED, (event) => cb(event.payload));
}

/** Hear Use the theme's colors for MUD text change. */
export function subscribeThemeTerminalColorsChanged(
  cb: (on: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(THEME_TERMINAL_COLORS_CHANGED, (event) => cb(event.payload));
}

/** Hear Command color change, null for the default. */
export function subscribeInputEchoColorChanged(
  cb: (color: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>(INPUT_ECHO_COLOR_CHANGED, (event) => cb(event.payload));
}

/** Hear Show the commands your macros send change. */
export function subscribeEchoMacrosChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(ECHO_MACROS_CHANGED, (event) => cb(event.payload));
}

/** Hear Wait between pasted lines change, in ms. */
export function subscribePasteLineDelayChanged(cb: (ms: number) => void): Promise<UnlistenFn> {
  return listen<number>(PASTE_LINE_DELAY_CHANGED, (event) => cb(event.payload));
}

/** Hear Use the same mark in the command line change. */
export function subscribeInputLineMarkChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(INPUT_LINE_MARK_CHANGED, (event) => cb(event.payload));
}

/** Hear Check spelling when you chat change. */
export function subscribeSpellcheckPromptChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(SPELLCHECK_PROMPT_CHANGED, (event) => cb(event.payload));
}

/** Hear Offer the card when the game's editor opens change. */
export function subscribeWritingOfferChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(WRITING_OFFER_CHANGED, (event) => cb(event.payload));
}

/** Hear Ask before you post change. */
export function subscribeWritingAskPostChanged(cb: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>(WRITING_ASK_POST_CHANGED, (event) => cb(event.payload));
}

/** Hear Caret shape change. */
export function subscribeInputCursorStyleChanged(cb: (style: string) => void): Promise<UnlistenFn> {
  return listen<string>(INPUT_CURSOR_STYLE_CHANGED, (event) => cb(event.payload));
}
