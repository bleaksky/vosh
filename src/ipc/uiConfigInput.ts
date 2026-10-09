// The command line as the config holds it, its caret, its look, the
// mark before your commands and the coloring as you type, and the
// events that bring a new pick.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  INPUT_ECHO_MARK_CHANGED,
  INPUT_LINE_LOOK_CHANGED,
  INPUT_TYPE_COLORS_CHANGED,
} from './events';
import type { UiConfig } from './uiConfig';
import { normalizeTextSize } from '../lib/textSize';

/** Caret shapes the command line can paint. Each one renders inside the
 *  same anchor box as the default block, so switching shapes never
 *  reflows the input row. */
export const INPUT_CURSOR_STYLES = [
  'block',
  'block_outline',
  'half_block',
  'underline',
  'underline_thick',
  'pipe',
  'pipe_thick',
] as const;

export type InputCursorStyle = (typeof INPUT_CURSOR_STYLES)[number];

/** Coerce an unknown caret shape back to the default block. */
export function normalizeInputCursorStyle(value: unknown): InputCursorStyle {
  return INPUT_CURSOR_STYLES.includes(value as InputCursorStyle)
    ? (value as InputCursorStyle)
    : 'block';
}

/** The command line's background: the theme's, a slight tint of the
 *  accent, or your own color. */
export const INPUT_LINE_BACKGROUNDS = ['theme', 'tint', 'own'] as const;

export type InputLineBackground = (typeof INPUT_LINE_BACKGROUNDS)[number];

/** Coerce an unknown background back to the theme's. */
export function normalizeInputLineBackground(value: unknown): InputLineBackground {
  return INPUT_LINE_BACKGROUNDS.find((pick) => pick === value) ?? 'theme';
}

/** What the command line Size row saves to follow your terminal size. */
export const INPUT_LINE_SIZE_TERMINAL = 0;

/** A saved command line size as the page reads it, on the nearest half
 *  step and held to 6 to 64 as Rust holds it. Anything but a number
 *  follows the terminal size. */
export function normalizeInputLineSize(value: unknown): number {
  return normalizeTextSize(value, INPUT_LINE_SIZE_TERMINAL, true);
}

/** A saved color, or null for an empty or missing one. */
export function optionalColor(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** The mark the echo of a command you send starts with: none, `›`,
 *  `>`, or your own text. */
export const INPUT_ECHO_MARKS = ['off', 'chevron', 'gt', 'own'] as const;

export type InputEchoMark = (typeof INPUT_ECHO_MARKS)[number];

/** Coerce an unknown mark back to `›`. */
export function normalizeInputEchoMark(value: unknown): InputEchoMark {
  return INPUT_ECHO_MARKS.find((mark) => mark === value) ?? 'chevron';
}

/** The mark your echo starts with and whether the command after it
 *  draws faint, sent to every window as one. */
export interface EchoMarkOptions {
  mark: InputEchoMark;
  /** Your own text, drawn only while `mark` is `own`. */
  text: string;
  /** Hex color of the mark, or null for the theme's bright black. */
  color: string | null;
  dim: boolean;
}

/** Your own mark keeps at most four characters, as Rust does. */
export const ECHO_MARK_TEXT_MAX = 4;

/** Your own mark as Rust saves it, so every window and the native grid
 *  draw the same bytes. Control characters drop, the ends trim, and it
 *  keeps at most four characters. */
export function coerceEchoMarkText(text: string): string {
  const clean = [...text].filter((ch) => !/\p{Cc}/u.test(ch)).join('');
  return [...clean.trim()].slice(0, ECHO_MARK_TEXT_MAX).join('').trimEnd();
}

/** The echo mark options as UiConfig keeps them. */
export function echoMarkOptionsOf(
  config: Pick<
    UiConfig,
    'input_echo_mark' | 'input_echo_mark_text' | 'input_echo_mark_color' | 'input_echo_dim'
  >,
): EchoMarkOptions {
  return {
    mark: config.input_echo_mark,
    text: coerceEchoMarkText(config.input_echo_mark_text),
    color: config.input_echo_mark_color,
    dim: config.input_echo_dim,
  };
}

/** Read echo mark options off the bus, filling anything missing or
 *  unknown with the defaults. */
export function normalizeEchoMarkOptions(raw: unknown): EchoMarkOptions {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    mark: normalizeInputEchoMark(o.mark),
    text: typeof o.text === 'string' ? coerceEchoMarkText(o.text) : '',
    color: typeof o.color === 'string' && o.color.length > 0 ? o.color : null,
    dim: o.dim === true,
  };
}

/** The echo mark options of a profile that never changed them. */
export const DEFAULT_ECHO_MARK_OPTIONS: EchoMarkOptions = normalizeEchoMarkOptions({});

/** How the command line looks, sent to every window as one. */
export interface LineLook {
  /** The caret blinks. Reduce motion still holds it steady. */
  blink: boolean;
  /** Hex color of the caret, or null for the theme accent. */
  caretColor: string | null;
  /** Hex color of what you type, or null for the theme text. */
  textColor: string | null;
  background: InputLineBackground;
  /** Your own background color, kept while another background is
   *  picked. */
  backgroundColor: string | null;
  /** Size in px of what you type, 0 for your terminal size. */
  size: number;
}

/** The command line look as UiConfig keeps it. */
export function lineLookOf(
  config: Pick<
    UiConfig,
    | 'input_caret_blink'
    | 'input_caret_color'
    | 'input_line_color'
    | 'input_line_background'
    | 'input_line_background_color'
    | 'input_line_size'
  >,
): LineLook {
  return {
    blink: config.input_caret_blink,
    caretColor: config.input_caret_color,
    textColor: config.input_line_color,
    background: config.input_line_background,
    backgroundColor: config.input_line_background_color,
    size: config.input_line_size,
  };
}

/** Read a command line look off the bus, filling anything missing or
 *  unknown with the defaults. */
function normalizeLineLook(raw: unknown): LineLook {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    blink: o.blink !== false,
    caretColor: optionalColor(o.caretColor),
    textColor: optionalColor(o.textColor),
    background: normalizeInputLineBackground(o.background),
    backgroundColor: optionalColor(o.backgroundColor),
    size: normalizeInputLineSize(o.size),
  };
}

/** The command line look of a profile that never changed it. */
export const DEFAULT_LINE_LOOK: LineLook = normalizeLineLook({});

/** Color commands as you type and its four colors, sent to every window
 *  as one. Each color is null for the theme's own. */
export interface TypeColors {
  on: boolean;
  /** A line that starts with an alias, the theme's cyan by default. */
  alias: string | null;
  /** A line that starts with a Vosh # command, the theme's magenta. */
  hash: string | null;
  /** A chat line, the whole line, the theme's yellow. */
  chat: string | null;
  /** A # command Vosh does not know, the theme's danger color. */
  unknown: string | null;
}

/** The coloring as you type as UiConfig keeps it. */
export function typeColorsOf(
  config: Pick<
    UiConfig,
    | 'input_type_colors'
    | 'input_type_alias_color'
    | 'input_type_hash_color'
    | 'input_type_chat_color'
    | 'input_type_unknown_color'
  >,
): TypeColors {
  return {
    on: config.input_type_colors,
    alias: config.input_type_alias_color,
    hash: config.input_type_hash_color,
    chat: config.input_type_chat_color,
    unknown: config.input_type_unknown_color,
  };
}

/** Read the coloring as you type off the bus, off and the theme colors
 *  for anything missing. */
function normalizeTypeColors(raw: unknown): TypeColors {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    on: o.on === true,
    alias: optionalColor(o.alias),
    hash: optionalColor(o.hash),
    chat: optionalColor(o.chat),
    unknown: optionalColor(o.unknown),
  };
}

/** The coloring as you type of a profile that never changed it. */
export const DEFAULT_TYPE_COLORS: TypeColors = normalizeTypeColors({});

/** Hear Mark before your commands, Mark color or Dim sent commands change. */
export function subscribeInputEchoMarkChanged(
  cb: (options: EchoMarkOptions) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(INPUT_ECHO_MARK_CHANGED, (event) =>
    cb(normalizeEchoMarkOptions(event.payload)),
  );
}

/** Hear Caret blinks, Caret color, Text color, Background or Size of
 *  the command line change. */
export function subscribeInputLineLookChanged(cb: (look: LineLook) => void): Promise<UnlistenFn> {
  return listen<unknown>(INPUT_LINE_LOOK_CHANGED, (event) => cb(normalizeLineLook(event.payload)));
}

/** Hear Color commands as you type or one of its colors change. */
export function subscribeInputTypeColorsChanged(
  cb: (colors: TypeColors) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(INPUT_TYPE_COLORS_CHANGED, (event) =>
    cb(normalizeTypeColors(event.payload)),
  );
}
