// Drawing a prompt design and editing it piece by piece.

import { invoke } from '@tauri-apps/api/core';

/** A color in a span. Palette indexes resolve through the active theme. */
export type PromptSpanColor =
  | { kind: 'default' }
  | { kind: 'index'; index: number }
  | { kind: 'rgb'; r: number; g: number; b: number };

/** Where a piece of a design landed: `row` is the line from `%nl`, `col`
 *  the cell in it before any wrap, and `width` the cells it takes, a wide
 *  character two and a combining mark none (`cellWidth` in sgrCells.ts).
 *  The look is the one at the piece's first cell. */
export interface PromptSpan {
  piece: number;
  row: number;
  col: number;
  width: number;
  fg: PromptSpanColor;
  bg: PromptSpanColor;
  bold: boolean;
  italic: boolean;
  underline: boolean;
}

/** A drawn design. */
export interface PromptRendered {
  ansi: string;
  plain: string;
  rows: number;
  spans: PromptSpan[];
}

/** Values a preview draws in place of the live ones, by field in the
 *  form the samples take. `"?"` draws a value hidden and `null` absent.
 *  `lament` hides every value the song hides. */
export interface PromptOverrides {
  values?: Record<string, string | number | boolean | null>;
  lament?: boolean;
}

/** The previews the card's footer offers. */
export type PromptPreviewName = 'now' | 'low_health' | 'fight' | 'lament';

/** One design to draw, with live or sample values. `preview` draws one
 *  of the card's previews, and `overrides` go on top of it. */
export interface PromptRenderRequest {
  template: string;
  values?: 'live' | 'sample';
  preview?: PromptPreviewName | null;
  overrides?: PromptOverrides | null;
  /** Draw each value with nothing to show as its label, as the open
   *  card does. */
  placeholders?: boolean;
  /** The cells `%{right}` pushes the rest of its row against, such as a
   *  vitals footer's width. Left out, it pushes by one space. Only
   *  promptRenderMany sends it. */
  cols?: number;
}

/** Draw a design, with the values of a session, the selected one when
 *  it names none. */
export async function promptRender(
  request: PromptRenderRequest,
  session?: number,
): Promise<PromptRendered> {
  return invoke('prompt_render', {
    template: request.template,
    values: request.values ?? 'live',
    preview: request.preview ?? null,
    overrides: request.overrides ?? null,
    placeholders: request.placeholders ?? false,
    session,
  });
}

/** Draw several designs at once, such as the start list. */
export async function promptRenderMany(
  requests: PromptRenderRequest[],
  session?: number,
): Promise<PromptRendered[]> {
  return invoke('prompt_render_many', { requests, session });
}

/** What the open card shows on your prompt in place of the live render:
 *  one of its previews with values on top, the labels of values with
 *  nothing to show (`placeholders`), or the line the game sent while it
 *  reads your codes (`raw`). */
export interface PromptPreview {
  preview?: PromptPreviewName | null;
  overrides?: PromptOverrides | null;
  placeholders?: boolean;
  raw?: boolean;
}

/** Show a preview on your prompt in a session, or the live render again
 *  with null. The open row carries the live render as its restore, so
 *  only live renders reach history. Nothing saves or goes to the game. */
export async function promptPreviewSet(
  preview: PromptPreview | null,
  session?: number,
): Promise<void> {
  await invoke('prompt_preview_set', { preview, session });
}

export type PromptFormatName =
  | 'value'
  | 'cur_max'
  | 'max'
  | 'pct'
  | 'pct_game'
  | 'percent'
  | 'bar'
  | 'game'
  | 'word'
  | 'ampm'
  | 'name'
  | 'grouped'
  | 'short'
  | 'thousands'
  | 'unit'
  | 'since'
  | 'trunc'
  | 'hm'
  | 'hms'
  | 'md'
  | 'count'
  | 'names'
  | 'on'
  | 'off';

/** A color the card offers. By value with no field is the piece's own
 *  value, by how full it is, by the game's `%h` bands with `game`, or in
 *  eleven steps from red to green with `steps`. */
export type PromptColorChoice =
  | { kind: 'default' }
  | { kind: 'named'; index: number }
  | { kind: 'index'; index: number }
  | { kind: 'rgb'; r: number; g: number; b: number }
  | { kind: 'by_value'; field?: string; game?: boolean; steps?: boolean };

/** How a value shows. A bar takes a width of 1 to 80 cells and a color,
 *  `trunc` how many characters it keeps. */
export interface PromptFormatChoice {
  format: PromptFormatName;
  width?: number;
  color?: PromptColorChoice;
  chars?: number;
}

/** The kinds of underline: `underline` is the single line (SGR 4), and
 *  `double`, `curly`, `dotted` and `dashed` are SGR 4:2 to 4:5. One kind
 *  holds at a time. */
export type PromptUnderlineStyle = 'underline' | 'double' | 'curly' | 'dotted' | 'dashed';

/** A style the card turns on or off. An underline kind replaces the kind
 *  a part had, and turning any underline kind off ends the underline. */
export type PromptStyleChoice =
  | 'bold'
  | 'dim'
  | 'italic'
  | PromptUnderlineStyle
  | 'inverse'
  | 'strike'
  | 'blink';

export type PromptWhen = 'always' | 'fight' | 'not_fight';

/** One change to a design. `piece` indexes the template's pieces, as a
 *  span names them. `at` and `to` are places between pieces, from 0
 *  before the first to the number of pieces after the last. */
export type PromptEditOp =
  | { op: 'set_format'; piece: number; format: PromptFormatChoice }
  | {
      op: 'set_color';
      piece: number;
      color: PromptColorChoice;
      background?: boolean;
      underline?: boolean;
    }
  | { op: 'set_style'; piece: number; style: PromptStyleChoice; on: boolean }
  | { op: 'set_when'; piece: number; when: PromptWhen }
  | { op: 'set_text'; piece: number; text: string }
  | { op: 'remove'; piece: number }
  | { op: 'insert_field'; at: number; field: string; format?: PromptFormatChoice }
  | { op: 'insert_text'; at: number; text: string }
  | { op: 'insert_nl'; at: number }
  | { op: 'insert_right'; at: number }
  | { op: 'move'; piece: number; to: number };

/** A design after an edit, drawn with the live values and placeholders. */
export interface PromptEdited {
  template: string;
  rendered: PromptRendered;
  /** Where the piece the edit acted on sits now: the piece it changed or
   *  moved, or the one it added. Null after a removal. */
  piece: number | null;
}

/** Apply one edit to a design. Every other piece keeps its look. Save
 *  the result with promptConfigSet. */
export async function promptEdit(
  template: string,
  op: PromptEditOp,
  session?: number,
): Promise<PromptEdited> {
  return invoke('prompt_edit', { template, op, session });
}

/** What a piece of a design holds, as the card names it. */
export type PromptPieceKind =
  | 'codes'
  | 'text'
  | 'value'
  | 'cur_max'
  | 'percent'
  | 'nl'
  | 'right'
  | 'raw'
  | 'if'
  | 'if_not'
  | 'end'
  | 'unknown';

/** One form a value takes. `segment` is what its Show as segment reads,
 *  the sample when it is short and else the name. */
export interface PromptForm {
  format: PromptFormatName;
  label: string;
  segment: string;
  sample: PromptRendered;
  /** Show as offers it. The picker offers every form. */
  show_as: boolean;
}

/** One piece of a design as the card shows it. `text` is its template
 *  text with its own codes. A max alone reads as its gauge's field in the
 *  form `max`. The colors and styles are its look at its first cell, a
 *  bar's color its cells'. `underline_style` names the kind of underline,
 *  null with none, and `underline_color` its color, `default` for the
 *  text's own. `when_fixed` says a fight condition outside another one
 *  holds it. */
export interface PromptPiece {
  piece: number;
  kind: PromptPieceKind;
  text: string;
  field: string | null;
  label: string;
  format: PromptFormatName | null;
  width: number | null;
  when: PromptWhen;
  when_fixed: boolean;
  color: PromptColorChoice;
  background: PromptColorChoice;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
  underline_style: PromptUnderlineStyle | null;
  underline_color: PromptColorChoice;
  inverse: boolean;
  strike: boolean;
  blink: boolean;
  literal: string | null;

  meta: string | null;
  forms: PromptForm[];
  by_value: boolean;
  shows: boolean;
}

export type PromptTokenKind = 'text' | 'code' | 'value' | 'condition' | 'line' | 'raw' | 'unknown';

/** One token of a design: where it sits in the text in UTF-16 units, the
 *  piece it belongs to, and whether Vosh knows the name it reads. */
export interface PromptToken {
  start: number;
  end: number;
  piece: number;
  kind: PromptTokenKind;
  name: string | null;
  known: boolean;
}

export interface PromptDescribed {
  pieces: PromptPiece[];
  tokens: PromptToken[];
}

/** What each piece and token of a design is, with what each value reads
 *  in the preview the card shows. */
export async function promptDescribe(
  template: string,
  preview: PromptPreviewName | null = null,
  overrides: PromptOverrides | null = null,
  session?: number,
): Promise<PromptDescribed> {
  return invoke('prompt_describe', { template, preview, overrides, session });
}

/** The forms a field takes, `hp` or `aff:sanctuary`, each drawn as the
 *  card shows it, for the picker. */
export async function promptForms(
  field: string,
  preview: PromptPreviewName | null = null,
  session?: number,
): Promise<PromptForm[]> {
  return invoke('prompt_forms', { field, preview, session });
}
