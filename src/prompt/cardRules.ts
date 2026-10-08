// The prompt card's decisions: which step it opens on, what its header
// and More menu offer, the copy that says where your codes came from,
// how the candidate box names each value, the rows the start list
// shows, how another game's numbers take names, and where the card sits
// over your prompt. Pure, so the components stay about layout.

import { profileDisplayName } from '../lib/characterProfiles';
import { listJoin, possessive } from '../lib/text';
import type { MoveMade } from './promptPieces';
import { parseSgrCells, textCells, type Cell } from '../terminal/sgrCells';
import type { SessionIdentity } from '../ipc/characters';
import type {
  PromptCapture,
  PromptCaptureCheck,
  PromptCaptureSource,
  PromptCheckRead,
  PromptConfig,
  PromptDesign,
  PromptLastSeen,
  PromptLineNumber,
  PromptPreset,
  PromptShow,
  PromptShowState,
} from '../ipc/prompt';
import type { GamePromptSeen } from '../stores/gmcp/gamePromptStore';

/** The card's steps. `codes-entry` asks for your setting, `codes` reads
 *  the codes the game sent, `point` asks which line is your prompt on
 *  another game and `name` names its numbers, `start` is the start list
 *  on first use, and `rest` the card at rest on every later open. */
export type CardStep = 'codes-entry' | 'codes' | 'point' | 'name' | 'start' | 'rest';

/** True when the profile reads a prompt. */
export function hasCapture(capture: PromptCapture): boolean {
  return capture.kind !== 'none';
}

/** The pattern the old capture trigger left, which the codes replace. */
function isMigrated(capture: PromptCapture): boolean {
  return capture.kind === 'regex' && capture.source === 'migrated';
}

/** Where the card opens. With no capture it asks for one: on The
 *  Forsaken Lands it reads the codes the game sent, or asks for them
 *  when the game sent none this session, and on any other game it
 *  points at the line. The pattern your old capture trigger left counts
 *  as none there, so the codes replace it. Otherwise it rests. */
export function openingStep(input: {
  capture: PromptCapture;
  forsaken: boolean;
  gameSent: boolean;
}): CardStep {
  const { capture, forsaken, gameSent } = input;
  if (hasCapture(capture) && !(forsaken && isMigrated(capture))) return 'rest';
  if (!forsaken) return 'point';
  return gameSent ? 'codes' : 'codes-entry';
}

/** Where Use Forsaken Lands prompt codes… goes on another host: the
 *  codes the game sent this session, as on the game's own host, or your
 *  setting when it sent none. */
export function codeReaderStep(gameSent: boolean): CardStep {
  return openingStep({ capture: { kind: 'none' }, forsaken: true, gameSent });
}

export type MoreItemId = 'change-codes' | 'point' | 'use-codes' | 'forget' | 'customize-vitals';

export type MoreItem = { id: MoreItemId; label: string; danger?: boolean } | 'separator';

const CHANGE_CODES: MoreItem = { id: 'change-codes', label: 'Change codes…' };
const POINT: MoreItem = { id: 'point', label: 'Point at the line instead…' };
const USE_CODES: MoreItem = { id: 'use-codes', label: 'Use Forsaken Lands prompt codes…' };
const FORGET: MoreItem = { id: 'forget', label: "Forget your game's prompt", danger: true };

/** The card's title, which names it to a reader too, and the name of
 *  its More button. A vitals text has its own title, which keeps it
 *  apart from Customize vitals…, which opens Settings. */
export function cardNames(kind: 'prompt' | 'vitals'): { title: string; options: string } {
  return kind === 'vitals'
    ? { title: 'Your vitals text', options: 'Vitals text options' }
    : { title: 'Customize prompt', options: 'Prompt options' };
}

/** What More offers on the vitals text card, which reads no prompt:
 *  Customize vitals…, which opens Settings there. */
export const VITALS_MORE: readonly MoreItem[] = [
  { id: 'customize-vitals', label: 'Customize vitals…' },
];

/** What More in the card's header offers. On The Forsaken Lands: Change
 *  codes… only when the game sent no prompt setting this session, then
 *  Point at the line instead…. On another game: the code reader. Then
 *  Forget your game's prompt after a separator, once the profile has a
 *  capture. The capture steps that read codes offer nothing, and
 *  pointing at the line offers no second way to point at it. */
export function moreItems(input: {
  step: CardStep;
  forsaken: boolean;
  gameSent: boolean;
  capture: PromptCapture;
}): MoreItem[] {
  const { step, forsaken, gameSent, capture } = input;
  if (step === 'codes' || step === 'codes-entry') return [];
  const items: MoreItem[] = [];
  if (forsaken) {
    if (!gameSent) items.push(CHANGE_CODES);
    if (step !== 'point' && step !== 'name') items.push(POINT);
  } else {
    items.push(USE_CODES);
  }
  if (hasCapture(capture)) {
    if (items.length > 0) items.push('separator');
    items.push(FORGET);
  }
  return items;
}

/** The buttons in the header beside Close: Edit as text once the card is
 *  past the capture steps, and More while it has anything to offer. */
export function headerButtons(
  step: CardStep,
  more: readonly MoreItem[],
): { editAsText: boolean; more: boolean } {
  const design = step === 'start' || step === 'rest';
  return { editAsText: design, more: design || more.length > 0 };
}

/** `Saved for` the logged in character when it owns the profile, else
 *  for the profile by its display name. */
export function savedForName(identity: SessionIdentity | null, active: string): string {
  const character = identity?.character?.trim();
  if (character && identity?.claimed_by === active) return `Saved for ${character}`;
  return `Saved for ${profileDisplayName(active)}`;
}

/** A time as #prompt says it, `5:04`, with no day half. */
export function clockTime(at: Date): string {
  const hour = at.getHours() % 12 || 12;
  return `${hour}:${String(at.getMinutes()).padStart(2, '0')}`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function sameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** The source line on the codes step while the game sent your codes. */
export function codesSourceLine(game: GamePromptSeen | null): string | null {
  if (!game) return null;
  const when = game.atLogin ? 'when you logged in' : `at ${clockTime(new Date(game.receivedAt))}`;
  return `The game sent these codes ${when}. Change them in the game and Vosh follows.`;
}

/** Where Vosh saw your setting when the game sent none this session: the
 *  reply to prompt you typed today, or your log from an earlier day. */
export function lastSeenLine(seen: PromptLastSeen | null, now: Date): string | null {
  if (!seen?.at) return null;
  const at = new Date(seen.at);
  if (Number.isNaN(at.getTime())) return null;
  if (sameDay(at, now)) return `Vosh saw it when you typed prompt at ${clockTime(at)}.`;
  return `Vosh found it in your log from ${MONTHS[at.getMonth()]} ${at.getDate()}.`;
}

/** The title and body of the setting step: Vosh saw your setting, it
 *  shows the codes the profile holds for Change codes…, or it asks for
 *  your setting. */
export function entryCopy(seenLine: string | null, saved = false): { title: string; body: string } {
  if (seenLine) {
    return {
      title: 'Is this your prompt setting?',
      body: `${seenLine} It reads the codes, so you never write a pattern.`,
    };
  }
  if (saved) {
    return {
      title: 'Is this your prompt setting?',
      body: 'Vosh reads the codes, so you never write a pattern.',
    };
  }
  return {
    title: 'What is your prompt setting?',
    body: 'Type prompt in the game and Vosh reads the answer. You can also paste it here.',
  };
}

/** The capture Use these codes saves. */
export function savedCapture(
  report: { prompt: string; fprompt: string },
  source: PromptCaptureSource,
  seenAt: string,
): PromptCapture {
  return {
    kind: 'aabahran',
    prompt: report.prompt,
    fprompt: report.fprompt,
    follow_game: true,
    seen_at: seenAt,
    source,
  };
}

/** A capture saved for the first time in this profile: before it the
 *  profile read nothing, or only the pattern the old trigger left. */
export function firstCapture(was: PromptCapture): boolean {
  return was.kind === 'none' || (was.kind === 'regex' && was.source === 'migrated');
}

/** The table once the card saves capture `next`. The draw switch stays
 *  as you set it, so reading your prompt never starts drawing your own.
 *  Only Draw your prompt, the Settings switch, the palette or
 *  `#prompt draw on` turns it on. */
export function withCapture(config: PromptConfig, next: PromptCapture): PromptConfig {
  return { ...config, capture: next };
}

/** `at` as RFC 3339 local time with its offset, as the backend writes
 *  `seen_at`. */
export function localStamp(at: Date): string {
  const pad = (n: number) => String(Math.abs(Math.trunc(n))).padStart(2, '0');
  const offset = -at.getTimezoneOffset();
  const sign = offset >= 0 ? '+' : '-';
  return (
    `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}` +
    `T${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}` +
    `${sign}${pad(offset / 60)}:${pad(offset % 60)}`
  );
}

// ---------------------------------------------------------------------
// The candidate box
// ---------------------------------------------------------------------

/** The candidate box's geometry: 10 above the first line, 17.5 per
 *  line, its names 4 under it in rows 15 tall and 2 apart, 6 between
 *  lines, 10.5 under the last. */
const BOX_TOP = 10;
const LINE_H = 17.5;
const NAME_GAP = 4;
const NAME_ROW = 15;
const NAME_ROW_GAP = 2;
const LINE_GAP = 6;
const BOX_BOTTOM = 10.5;
/** A name closer than this to the one before it takes the second row. */
const NAME_SPACE = 8;
/** The text's left edge in the box. */
export const BOX_TEXT_X = 10;

export interface PlacedLabel {
  /** The mark's index in the read. */
  mark: number;
  label: string;
  left: number;
  top: number;
  row: 0 | 1;
  warn: boolean;
}

export interface PlacedLabels {
  labels: PlacedLabel[];
  /** The top of each line in the box. */
  lineTops: number[];
  /** How many rows of names each line takes, 0 to 2. */
  nameRows: number[];
}

/** The cells before character `index` of `line`, a wide character two. */
export function cellsBefore(line: string, index: number): number {
  return textCells(Array.from(line).slice(0, index).join(''));
}

/** Where each name goes under the line it names: under its first
 *  character, `cellW` per cell from the box's text edge. A name that
 *  would come within 8 px of the one before it on the first row takes
 *  the second row. */
export function placeLabels(
  read: PromptCheckRead,
  cellW: number,
  measure: (label: string) => number,
): PlacedLabels {
  const lines = read.plain.split('\n');
  const labels: PlacedLabel[] = [];
  const nameRows: number[] = lines.map(() => 0);
  const lineTops: number[] = [];
  const rights: number[] = lines.map(() => Number.NEGATIVE_INFINITY);
  const perLine: PlacedLabel[][] = lines.map(() => []);
  read.marks.forEach((mark, index) => {
    const line = lines[mark.line] ?? '';
    const left = BOX_TEXT_X + cellsBefore(line, mark.start) * cellW;
    const width = measure(mark.label);
    const first = left >= rights[mark.line] + NAME_SPACE;
    if (first) rights[mark.line] = left + width;
    const row: 0 | 1 = first ? 0 : 1;
    nameRows[mark.line] = Math.max(nameRows[mark.line], row + 1);
    const placed: PlacedLabel = {
      mark: index,
      label: mark.label,
      left,
      top: 0,
      row,
      warn: mark.warn,
    };
    perLine[mark.line]?.push(placed);
    labels.push(placed);
  });
  let top = BOX_TOP;
  lines.forEach((_, i) => {
    lineTops.push(top);
    const namesTop = top + LINE_H + NAME_GAP;
    for (const label of perLine[i]) {
      label.top = namesTop + label.row * (NAME_ROW + NAME_ROW_GAP);
    }
    top += lineHeight(nameRows[i]) + LINE_GAP;
  });
  return { labels, lineTops, nameRows };
}

function lineHeight(rows: number): number {
  if (rows === 0) return LINE_H;
  return LINE_H + NAME_GAP + NAME_ROW + (rows > 1 ? NAME_ROW_GAP + NAME_ROW : 0);
}

/** The box's height for the lines and names placed. */
export function boxHeight(placed: PlacedLabels): number {
  const lines = placed.nameRows.reduce((sum, rows) => sum + lineHeight(rows), 0);
  return BOX_TOP + lines + LINE_GAP * Math.max(0, placed.nameRows.length - 1) + BOX_BOTTOM;
}

/** The rows of a prompt as the game sent it, as cells. */
export function readRows(read: PromptCheckRead): Cell[][] {
  const rows = parseSgrCells(read.raw);
  const lines = read.plain.split('\n');
  while (rows.length < lines.length) rows.push([]);
  return rows.slice(0, lines.length);
}

/** Each line's marks in cell columns. */
export function readMarks(read: PromptCheckRead): { from: number; to: number; warn: boolean }[][] {
  const lines = read.plain.split('\n');
  return lines.map((line, i) =>
    read.marks
      .filter((m) => m.line === i)
      .map((m) => ({
        from: cellsBefore(line, m.start),
        to: cellsBefore(line, m.end),
        warn: m.warn,
      })),
  );
}

/** The legend's two columns, filled down the first and then the second,
 *  the first taking the odd row. */
export function legendColumns<T>(rows: readonly T[]): [T[], T[]] {
  const half = Math.ceil(rows.length / 2);
  return [rows.slice(0, half), rows.slice(half)];
}

/** The note that names the immortal prefix the prompt shows. */
export function prefixNote(read: PromptCheckRead | null): string | null {
  if (!read) return null;
  const lines = read.plain.split('\n');
  const parts: string[] = [];
  for (const [field, word] of [
    ['wizi', 'Wizi'],
    ['incog', 'Incog'],
  ] as const) {
    const mark = read.marks.find((m) => m.field === field);
    if (!mark) continue;
    const value = Array.from(lines[mark.line] ?? '')
      .slice(mark.start, mark.end)
      .join('');
    parts.push(`(${word} ${value})`);
  }
  if (parts.length === 0) return null;
  const both = parts.length > 1;
  return `The game puts ${listJoin(parts)} in front. Vosh reads ${both ? 'those' : 'that'} too.`;
}

/** The note that says the codes replace the pattern your old capture
 *  trigger left. */
export function migratedNote(capture: PromptCapture): string | null {
  return isMigrated(capture) ? 'This replaces the pattern from your old capture trigger.' : null;
}

/** The match line, a sentence to a line beside the stepper. */
export function matchSentences(text: string): string[] {
  return text
    .split(/(?<=\.)\s+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** The lines of the match copy: each sentence on its own line, so
 *  the fight count sits under the match, but the empty ring copy as one
 *  paragraph that wraps. */
export function matchLines(check: PromptCaptureCheck): string[] {
  return check.total === 0 ? [check.text] : matchSentences(check.text);
}

/** How the match line reads: a check for a clean match, a warn dot for a
 *  poor one, and nothing before the first prompt. */
export function matchTone(check: PromptCaptureCheck): 'ok' | 'warn' | 'none' {
  if (check.total === 0) return 'none';
  return check.matched > 0 && check.false_matches === 0 ? 'ok' : 'warn';
}

// ---------------------------------------------------------------------
// The start list
// ---------------------------------------------------------------------

export interface StartRow {
  id: string;
  label: string;
  template: string;
  checked: boolean;
}

/** The rows of the start list and the Presets menu (section 7 step 5):
 *  Vosh's default, Yours, Your design before that when it exists, then
 *  the presets in their order, the designs other profiles hold for From
 *  another profile, and Start empty. An earlier design the same as the
 *  default or as the row above it is left out. While your design follows
 *  the game, Same as the game takes the check. Otherwise the first other
 *  row that holds your design takes it. */
export function startRows(
  presets: readonly PromptPreset[],
  config: PromptConfig,
  designs: readonly PromptDesign[],
): { rows: StartRow[]; others: StartRow[]; empty: StartRow | null } {
  const row = (id: string, label: string, template: string): StartRow => ({
    id,
    label,
    template,
    checked: false,
  });
  const fallback = presets.find((p) => p.id === 'default');
  const rows: StartRow[] = [];
  if (fallback) rows.push(row('default', fallback.label, fallback.template));
  const [yours, before] = config.previous_templates;
  const kept = (t: string | undefined, not: (string | undefined)[]): t is string =>
    t !== undefined && t.length > 0 && !not.includes(t);
  if (kept(yours, [fallback?.template])) rows.push(row('yours', 'Yours', yours));
  if (kept(before, [fallback?.template, yours])) {
    rows.push(row('before', 'Your design before that', before));
  }
  for (const preset of presets) {
    if (preset.id === 'default' || preset.id === 'empty') continue;
    rows.push(row(preset.id, preset.label, preset.template));
  }
  const others = designs.map((d) =>
    row(
      `profile:${d.profile}`,
      `${possessive(profileDisplayName(d.display_name))} prompt`,
      d.template,
    ),
  );
  const emptyPreset = presets.find((p) => p.id === 'empty');
  const empty = emptyPreset ? row('empty', emptyPreset.label, '') : null;
  const all = [...rows, ...others, ...(empty ? [empty] : [])];
  // A design that follows the game is empty only while there are no
  // codes to follow, and then no row holds it.
  const first = config.mirror
    ? config.template
      ? all.find((r) => r.id === 'game')
      : undefined
    : all.find((r) => r.id !== 'game' && r.template === config.template);
  if (first) first.checked = true;
  return { rows, others, empty };
}

/** The Presets of the vitals text card: Vosh's text, Yours, Your text
 *  before that from the earlier texts, and Your 0.7 text while your 0.7
 *  template was on. A text the same as a row above it is left out, and
 *  the first row that holds your text takes the check. */
export function vitalsStartRows(
  config: PromptConfig,
  vosh: string,
  legacy: string | null,
): { rows: StartRow[]; others: StartRow[]; empty: null } {
  const [yours, before] = config.previous_templates;
  const rows: StartRow[] = [];
  const add = (id: string, label: string, template: string | null | undefined) => {
    if (template && !rows.some((row) => row.template === template)) {
      rows.push({ id, label, template, checked: false });
    }
  };
  add('default', "Vosh's text", vosh);
  add('yours', 'Yours', yours);
  add('before', 'Your text before that', before);
  add('legacy', 'Your 0.7 text', legacy);
  const first = rows.find((row) => row.template === config.template);
  if (first) first.checked = true;
  return { rows, others: [], empty: null };
}

// ---------------------------------------------------------------------
// Naming another game's numbers
// ---------------------------------------------------------------------

export interface NumberButton {
  /** The number it names, the first of a pair. */
  index: number;
  label: string;
  aria: string;
}

/** One button under each pair of a value and its max, and under each
 *  number on its own, named for the value it reads. */
export function numberButtons(numbers: readonly PromptLineNumber[]): NumberButton[] {
  const out: NumberButton[] = [];
  numbers.forEach((n, i) => {
    if (n.max) return;
    const max = numbers[i + 1]?.max ? numbers[i + 1] : null;
    const named = n.name.length > 0 && !/^n\d+$/.test(n.name);
    const label = named ? n.label : n.name.length > 0 ? 'Name this number' : 'Left out';
    const what = max ? `${n.text} of ${max.text}` : n.text;
    const aria = named ? `${what} is ${n.label}` : `${what} is not named`;
    out.push({ index: i, label, aria });
  });
  return out;
}

/** The name buttons' geometry: the first row 35.5 down the box,
 *  each button 20 tall with its label 6 in from its left, a 2 px gap, a
 *  12 px chevron and 4 at its right, in a box 66 tall. A further row
 *  goes 2 under the one above, as the names on the codes step do. */
const NAME_BUTTON_TOP = 35.5;
const NAME_BUTTON_H = 20;
const NAME_BUTTON_ROW_GAP = 2;
const NAME_BUTTON_INSET = 6;
const NAME_BUTTON_CHROME = 6 + 2 + 12 + 4;
const NAME_BOX_H = 66;
/** A button closer than this to the one before it takes another row. Its
 *  own padding keeps the labels 14 px or more apart past the chevron. */
const NAME_BUTTON_SPACE = 4;

export interface PlacedNameButton {
  index: number;
  left: number;
  right: number;
  top: number;
}

/** Where each name button goes under the number it names: its label
 *  under the number's first cell, on the first row where it comes no
 *  closer than 4 px to the button before it, so the buttons for a
 *  Wizi, an Incog and the health beside them never draw over each other.
 *  Returns the box's height for the rows they take. */
export function placeNameButtons(
  buttons: readonly { index: number; col: number; label: string }[],
  cellW: number,
  measure: (label: string) => number,
): { buttons: PlacedNameButton[]; height: number } {
  const rights: number[] = [];
  const placed = buttons.map(({ index, col, label }) => {
    const left = BOX_TEXT_X + col * cellW - NAME_BUTTON_INSET;
    const right = left + measure(label) + NAME_BUTTON_CHROME;
    let row = rights.findIndex((edge) => left >= edge + NAME_BUTTON_SPACE);
    if (row < 0) row = rights.length;
    rights[row] = right;
    return {
      index,
      left,
      right,
      top: NAME_BUTTON_TOP + row * (NAME_BUTTON_H + NAME_BUTTON_ROW_GAP),
    };
  });
  const rows = Math.max(1, rights.length);
  return {
    buttons: placed,
    height: NAME_BOX_H + (rows - 1) * (NAME_BUTTON_H + NAME_BUTTON_ROW_GAP),
  };
}

/** The names of every number after naming the one at `index`, with its
 *  max when it is the first of a pair. An empty name leaves both out. */
export function namesFor(
  numbers: readonly PromptLineNumber[],
  index: number,
  name: string,
): string[] {
  const names = numbers.map((n) => n.name);
  names[index] = name;
  if (numbers[index + 1]?.max) names[index + 1] = name ? `max${name}` : '';
  return names;
}

export interface NameChoice {
  name: string;
  label: string;
  package?: string;
}

/** The name menu's choices: the vitals Vosh knows, then the values the
 *  game sends over GMCP that it has no name for. Other name… and Leave
 *  out follow them. */
export function nameChoices(gmcp: readonly { name: string; package: string }[]): NameChoice[][] {
  const known: NameChoice[] = [
    { name: 'hp', label: 'Health' },
    { name: 'mana', label: 'Mana' },
    { name: 'move', label: 'Moves' },
  ];
  const sent = gmcp.map((g) => ({ name: g.name, label: g.name, package: g.package }));
  return sent.length > 0 ? [known, sent] : [known];
}

// ---------------------------------------------------------------------
// Where the card sits
// ---------------------------------------------------------------------

/** How far above your prompt the card ends. */
const CARD_GAP = 4;
/** How far under the top of the terminal the card may reach. */
const CARD_TOP_MARGIN = 8;

/** Where the card ends, as CSS `bottom` from the window's bottom, and
 *  the most it may be tall. In the text and lifted it ends 4 px above
 *  the row right above your prompt's first row, so no row is cut in
 *  half next to the line you edit, and with no open row it sits as it
 *  would over a prompt on the last row. Pinned, it ends 4 px above the
 *  band's first row. */
export function cardAnchor(input: {
  pinned: boolean;
  /** The client y of the open row's first row. */
  promptTop: number | null;
  /** The client y of the terminal's last row. */
  lastRowTop: number;
  /** The client y of the pinned band's first row. */
  bandRowTop: number | null;
  cellH: number;
  /** The client y of the terminal area's top. */
  areaTop: number;
  viewportH: number;
}): { bottom: number; maxHeight: number } {
  const edge =
    input.pinned && input.bandRowTop !== null
      ? input.bandRowTop - CARD_GAP
      : (input.promptTop ?? input.lastRowTop) - input.cellH - CARD_GAP;
  return {
    bottom: input.viewportH - edge,
    maxHeight: Math.max(0, edge - (input.areaTop + CARD_TOP_MARGIN)),
  };
}

/** The vitals text card's gap to the panel and its foot's to the input
 *  band. */
const BESIDE_GAP = 12;
const ABOVE_INPUT = 16;

/** Where the vitals text card sits, as CSS `right` and `bottom` from the
 *  window's edges, and the most it may be tall: over the terminal 12 px
 *  from the panel, its foot just over the input band, so the footer it
 *  edits stays in view. The terminal area ends at the panel and at the
 *  input band. */
export function besideAnchor(input: {
  areaTop: number;
  areaRight: number;
  areaBottom: number;
  viewportW: number;
  viewportH: number;
}): { right: number; bottom: number; maxHeight: number } {
  const edge = input.areaBottom - ABOVE_INPUT;
  return {
    right: input.viewportW - input.areaRight + BESIDE_GAP,
    bottom: input.viewportH - edge,
    maxHeight: Math.max(0, edge - (input.areaTop + CARD_TOP_MARGIN)),
  };
}

// ---------------------------------------------------------------------
// Where its menus open
// ---------------------------------------------------------------------

/** Where a menu opens from its button: under it with their right edges
 *  together (More), above it with their left edges together (Presets),
 *  above it with their right edges together (the preview), under it with
 *  their left edges together (a name), or beside a row (From another
 *  profile). */
export type MenuPlace = 'below-end' | 'above-start' | 'above-end' | 'below-start' | 'beside';

/** How far a menu sits from its button, and from the window's edge. */
const GAP = 4;
const EDGE = 8;

/** Where a menu `size` goes beside `anchor`, kept inside the window. A
 *  menu that opens above or below its button opens on the other side
 *  when its own side has no room for it and the other side has. With
 *  room on neither side, it opens on the side with more and scrolls in
 *  `maxHeight`, so it never covers its button. */
export function menuPosition(
  anchor: { left: number; top: number; right: number; bottom: number },
  size: { width: number; height: number },
  place: MenuPlace,
  viewport: { width: number; height: number },
): { left: number; top: number; maxHeight?: number } {
  let left: number;
  let top: number;
  let height = size.height;
  let maxHeight: number | undefined;
  if (place === 'beside') {
    left = anchor.right + GAP;
    top = anchor.top - 6;
  } else {
    left = place === 'below-end' || place === 'above-end' ? anchor.right - size.width : anchor.left;
    const roomAbove = anchor.top - GAP - EDGE;
    const roomBelow = viewport.height - EDGE - anchor.bottom - GAP;
    const wantsAbove = place === 'above-start' || place === 'above-end';
    const [own, other] = wantsAbove ? [roomAbove, roomBelow] : [roomBelow, roomAbove];
    let above = wantsAbove;
    if (own < height) {
      if (other >= height || other > own) above = !wantsAbove;
      const room = Math.max(0, Math.max(own, other));
      if (room < height) {
        height = room;
        maxHeight = room;
      }
    }
    top = above ? anchor.top - GAP - height : anchor.bottom + GAP;
  }
  const pos = {
    left: Math.max(EDGE, Math.min(left, viewport.width - size.width - EDGE)),
    top: Math.max(EDGE, Math.min(top, viewport.height - height - EDGE)),
  };
  return maxHeight === undefined ? pos : { ...pos, maxHeight };
}

/** The table once you pick `row` in the start list or the Presets menu.
 *  Picking a start is how you ask Vosh to draw it, so drawing turns on.
 *  Same as the game follows the game from then on, and any other start
 *  makes the design yours. */
export function withStart(
  config: PromptConfig,
  row: Pick<StartRow, 'id' | 'template'>,
): PromptConfig {
  return { ...config, template: row.template, draw: true, mirror: row.id === 'game' };
}

/** The table once you edit the design. An edit makes the design yours,
 *  starting from the text it edited, even one that followed the game. */
export function withDesign(config: PromptConfig, template: string): PromptConfig {
  return { ...config, template, mirror: false };
}

/** The table once you pick where your prompt shows at the card's foot.
 *  Only the place changes. Command Z never takes it back, as it never
 *  takes back a place you picked in Settings, since the card moves with
 *  your prompt to its new place. */
export function withShow(config: PromptConfig, show: PromptShow): PromptConfig {
  return { ...config, show };
}

/** What the button at the card's foot reads of where your prompt
 *  shows. Right after your first capture the card's table holds it a
 *  round trip before the state reads it. The button waits quietly then,
 *  off with nothing to say, so it never asks you to customize the prompt
 *  you are customizing. */
export function cardShowState(
  show: PromptShowState | null,
  capture: PromptCapture,
): PromptShowState | null {
  return show && !show.capture && hasCapture(capture) ? null : show;
}

/** The table once the opposite Option key takes move `back` back. The
 *  design is the one before the move, and it follows the game again when
 *  it did then, as Command Z puts it back. */
export function withMoveTakenBack(
  config: PromptConfig,
  back: Pick<MoveMade, 'before' | 'mirror'>,
): PromptConfig {
  return { ...withDesign(config, back.before), mirror: back.mirror };
}

/** The table an edit of the design saves once its round trips land, or
 *  null when the design came out as it was. The edit began on `start`,
 *  and the table can change while it waits, as when you pick where your
 *  prompt shows or turn Draw your prompt off. So the new design goes on
 *  the table as it stands `now`, and those changes stay. */
export function editedTable(
  start: PromptConfig,
  now: PromptConfig | null,
  template: string,
): PromptConfig | null {
  if (template === start.template) return null;
  return withDesign(now ?? start, template);
}

/** The table once move `back` is taken back, on the table as it stands
 *  `now` for the same reason. */
export function movedBackTable(
  start: PromptConfig,
  now: PromptConfig | null,
  back: Pick<MoveMade, 'before' | 'mirror'>,
): PromptConfig {
  return withMoveTakenBack(now ?? start, back);
}

/** What Command Z puts back: the fields one change of yours made, as
 *  they were before it. */
export type UndoEntry = Partial<Pick<PromptConfig, 'template' | 'draw' | 'capture' | 'mirror'>>;

/** The entry that takes back the change from `before` to `next`, or null
 *  when it changed none of the design, whether it follows the game, the
 *  switch or the capture. Only those, so taking it back never puts back
 *  what changed elsewhere since: the codes the game sent, or where your
 *  prompt shows. */
export function undoEntry(before: PromptConfig, next: PromptConfig): UndoEntry | null {
  const entry: UndoEntry = {};
  if (before.template !== next.template) entry.template = before.template;
  if (before.mirror !== next.mirror) entry.mirror = before.mirror;
  if (before.draw !== next.draw) entry.draw = before.draw;
  if (JSON.stringify(before.capture) !== JSON.stringify(next.capture)) {
    entry.capture = before.capture;
  }
  return Object.keys(entry).length > 0 ? entry : null;
}

/** The table as it stands now with `entry` taken back. */
export function takeBackOnto(now: PromptConfig, entry: UndoEntry): PromptConfig {
  return { ...now, ...entry };
}

/** What a request to open the card asks for: the view to open on, the
 *  design's parts or its text, or pointing at your game's line. */
export type CardRequestView = 'design' | 'text' | 'point';

/** A request to open the card, counted so the card, open already, hears
 *  the same request again, such as Edit prompt as text… after you went
 *  back to the parts. */
export interface CardRequest {
  view: CardRequestView;
  at: number;
}

export function nextCardRequest(prev: CardRequest | null, view: CardRequestView): CardRequest {
  return { view, at: (prev?.at ?? 0) + 1 };
}

/** A sample cut to a column `column` px wide, as text-overflow ellipsis
 *  cuts it: whole when its cells fit, else the cells that fit with the
 *  ellipsis after them, with the hair a fit needs, and the column's
 *  whole width. */
export function sampleCut(
  total: number,
  cellW: number,
  column: number,
): { kept: number; width: number } {
  const round = (n: number) => Math.round(n * 100) / 100;
  if (total * cellW <= column + 0.01) return { kept: total, width: round(total * cellW) };
  return { kept: Math.max(0, Math.ceil(column / cellW - 1e-6) - 2), width: column };
}
