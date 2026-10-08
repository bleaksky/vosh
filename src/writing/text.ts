// The rules a line in the writing card follows: how wide the game reads
// it, how it flows as you type, how a paste and a rewrap break it, what
// the card marks on it, and how the footer counts it. Every line in the
// card is a line the game gets, so nothing here wraps softly or changes
// a word. It moves breaks and spaces, and folds only what the game
// would drop.

/** The most a text may hold in the game's editor, MDL less 4
 *  (olc.c:3832, merc.h:169). */
export const EDITOR_ROOM = 4604;

/** A line the editor reads as a command, not text: a dot runs a dot
 *  command, @ ends the editor, ! repeats your last line (olc.c:3613,
 *  olc.c:3746, comm.c:1560). */
export function startsAsCommand(line: string): boolean {
  return /^[.@!]/.test(line);
}

/** The color code that opens a line, `X with X anything but a second
 *  backtick, which the game keeps there (comm.c:1499). */
export function leadingCode(line: string): string | null {
  return line.length >= 2 && line[0] === '`' && line[1] !== '`' ? line[1] : null;
}

/** Where each code inside the line sits after the one at its start,
 *  as [from, to) over the line. */
export function innerCodes(line: string): [number, number][] {
  const out: [number, number][] = [];
  const start = leadingCode(line) ? 2 : 0;
  for (let i = start; i < line.length - 1; i += 1) {
    if (line[i] === '`') {
      out.push([i, i + 2]);
      i += 1;
    }
  }
  return out;
}

/** How many columns a line takes as a looker sees it. A code counts
 *  nothing, as the game measures it when it folds a line for a looker
 *  (comm.c:6919) and when it formats (olc.c:4224), and nor do spaces at
 *  its end, which go when Vosh sends it. */
export function columns(text: string): number {
  const line = text.replace(/ +$/, '');
  let n = line.length;
  if (leadingCode(line)) n -= 2;
  n -= innerCodes(line).length * 2;
  return n;
}

/** The offset in `line` of column `col`, codes skipped. */
export function offsetOfColumn(line: string, col: number): number {
  const codes = new Set<number>();
  if (leadingCode(line)) {
    codes.add(0);
    codes.add(1);
  }
  for (const [a, b] of innerCodes(line)) for (let i = a; i < b; i += 1) codes.add(i);
  let seen = 0;
  for (let i = 0; i < line.length; i += 1) {
    if (codes.has(i)) continue;
    if (seen === col) return i;
    seen += 1;
  }
  return line.length;
}

/** The lines as the game stores them, two bytes for each line end and
 *  one space for an empty line (olc.c:3834, comm.c:1506). */
export function storedBytes(lines: readonly string[]): number {
  let n = 0;
  for (const line of lines) {
    const kept = line.replace(/ +$/, '');
    n += (kept.length || 1) + 2;
  }
  return n;
}

/** The index of the first line the game's editor would refuse: a line
 *  goes in while what it holds and the line stay under EDITOR_ROOM. */
export function cutLine(lines: readonly string[], room = EDITOR_ROOM): number | null {
  let held = 0;
  for (let k = 0; k < lines.length; k += 1) {
    const kept = lines[k].replace(/ +$/, '') || ' ';
    if (held + kept.length >= room) return k;
    held += kept.length + 2;
  }
  return null;
}

// ── Folding what the game drops ──────────────────────────────────────

/** A character the game drops, folded to what it keeps, and how the
 *  footer names the fold. The game keeps only printable ASCII
 *  (comm.c:1501). */
const FOLDS: { from: RegExp; to: string; one: string; many: string }[] = [
  {
    from: /[\u2018\u2019\u201a\u201b\u2032]/g,
    to: "'",
    one: 'curly apostrophe',
    many: 'curly apostrophes',
  },
  { from: /[\u201c\u201d\u201e\u201f\u2033]/g, to: '"', one: 'curly quote', many: 'curly quotes' },
  { from: /\u2014/g, to: '--', one: 'long dash', many: 'long dashes' },
  { from: /[\u2013\u2012\u2212]/g, to: '-', one: 'short dash', many: 'short dashes' },
  { from: /\u2026/g, to: '...', one: 'ellipsis', many: 'ellipses' },
  { from: /[\u00a0\u2007\u202f\t]/g, to: ' ', one: 'wide space', many: 'wide spaces' },
];

/** What a fold changed, for the footer. */
export interface Folded {
  what: string;
  count: number;
}

/** `text` with the characters the game would drop folded to their plain
 *  forms, and what changed. */
export function fold(text: string): { text: string; folded: Folded[] } {
  let out = text;
  const folded: Folded[] = [];
  for (const rule of FOLDS) {
    const count = (out.match(rule.from) ?? []).length;
    if (count === 0) continue;
    out = out.replace(rule.from, rule.to);
    folded.push({ what: count === 1 ? rule.one : rule.many, count });
  }
  return { text: out, folded };
}

/** A character the game drops that no fold covers, or a double quote,
 *  which the game shows as a single one (db.c:7283). */
export function marksCharacter(c: string): 'quote' | 'dropped' | null {
  if (c === '"') return 'quote';
  const code = c.charCodeAt(0);
  if (code < 0x20 || code > 0x7e) return 'dropped';
  return null;
}

// ── Breaking lines ───────────────────────────────────────────────────

/** A line and whether it flows into the next one, a break Vosh made as
 *  it wrapped. A break you make with Return, and every break of a text
 *  read from the game, does not flow. */
export interface Row {
  text: string;
  flows: boolean;
}

/** Where the caret sits, by row and offset in it. */
export interface Caret {
  row: number;
  col: number;
}

/** A paragraph starts after an empty line or at a line that starts with
 *  spaces. */
function startsParagraph(line: string): boolean {
  return /^ /.test(line);
}

/** The words of `text` and the spaces before each, its leading spaces
 *  kept on the first. */
function words(text: string): { gap: string; word: string; at: number }[] {
  const out: { gap: string; word: string; at: number }[] = [];
  const re = /( *)([^ ]+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    out.push({ gap: m[1], word: m[2], at: m.index + m[1].length });
  }
  return out;
}

/** Break `text` at its spaces into lines of at most `width` columns.
 *  The spaces a break falls on stay at the end of the line before it,
 *  where they count nothing and go when Vosh sends, so the lines put
 *  together give `text` back. A word longer than `width` keeps a line
 *  of its own, since Vosh never cuts a word. A word that opens with a
 *  code starts a line, so the code stays at a line's start, and no line
 *  after the first starts with a dot, @ or !. Returns the lines and the
 *  offset in `text` each starts at. */
export function breakText(text: string, width: number): { lines: string[]; starts: number[] } {
  const ws = words(text);
  if (ws.length === 0) return { lines: [text], starts: [0] };
  const firsts = [0];
  let cols = ws[0].gap.length + columns(ws[0].word);
  for (let k = 1; k < ws.length; k += 1) {
    const w = ws[k];
    const code = leadingCode(w.word) !== null;
    if (!code && cols + w.gap.length + columns(w.word) <= width) {
      cols += w.gap.length + columns(w.word);
      continue;
    }
    const first = firsts[firsts.length - 1];
    // Take the word before along, so no line opens as a command.
    const at = !code && startsAsCommand(w.word) && k - first > 1 ? k - 1 : k;
    firsts.push(at);
    cols = 0;
    for (let n = at; n <= k; n += 1) cols += (n > at ? ws[n].gap.length : 0) + columns(ws[n].word);
  }
  const starts = firsts.map((first) => (first === 0 ? 0 : ws[first].at));
  const lines = starts.map((start, k) =>
    text.slice(start, k + 1 < starts.length ? starts[k + 1] : text.length),
  );
  return { lines, starts };
}

/** The rows of the paragraph that holds row `at`: the rows that flow
 *  into each other around it. */
function flowRun(rows: readonly Row[], at: number): [number, number] {
  let a = at;
  while (a > 0 && rows[a - 1].flows) a -= 1;
  let b = at;
  while (b < rows.length - 1 && rows[b].flows) b += 1;
  return [a, b];
}

/** Lay `text` out as rows from `start`, the last ending as `lastFlows`,
 *  and place the caret at `caretAt` in `text` when it is given. */
function layout(
  text: string,
  width: number,
  lastFlows: boolean,
  caretAt: number | null,
  start: number,
): { rows: Row[]; caret: Caret | null } {
  const { lines, starts } = breakText(text, width);
  const rows = lines.map((line, k) => ({ text: line, flows: k < lines.length - 1 || lastFlows }));
  let caret: Caret | null = null;
  if (caretAt !== null) {
    let k = starts.length - 1;
    while (k > 0 && starts[k] > caretAt) k -= 1;
    const col = Math.min(Math.max(0, caretAt - starts[k]), lines[k].length);
    caret = { row: start + k, col };
  }
  return { rows, caret };
}

/** Flow the paragraph that holds the caret after an edit, so each line
 *  of it fits `width` again: a line past it hands its last words down,
 *  and a short one takes words up from the rows that flow into it. The
 *  breaks you made stay. */
export function flow(
  rows: readonly Row[],
  caret: Caret,
  width: number,
): { rows: Row[]; caret: Caret } {
  const [a, b] = flowRun(rows, caret.row);
  let text = '';
  let caretAt = 0;
  for (let k = a; k <= b; k += 1) {
    if (k === caret.row) caretAt = text.length + caret.col;
    text += rows[k].text;
  }
  const laid = layout(text, width, rows[b].flows && b < rows.length - 1, caretAt, a);
  const next = [...rows.slice(0, a), ...laid.rows, ...rows.slice(b + 1)];
  return { rows: next, caret: laid.caret ?? caret };
}

/** The paragraphs of `rows` as Rewrap takes them: runs of rows with
 *  text, each run broken where an empty row ends it or a row that
 *  starts with spaces begins another. */
export function paragraphs(rows: readonly Row[]): [number, number][] {
  const out: [number, number][] = [];
  let open: number | null = null;
  rows.forEach((row, k) => {
    const empty = row.text.trim().length === 0;
    if (empty) {
      if (open !== null) out.push([open, k - 1]);
      open = null;
      return;
    }
    if (open !== null && startsParagraph(row.text)) {
      out.push([open, k - 1]);
      open = k;
      return;
    }
    if (open === null) open = k;
  });
  if (open !== null) out.push([open, rows.length - 1]);
  return out;
}

/** Rewrap the paragraph from `a` to `b` at `width`, keeping its indent,
 *  every word and the spaces between them. */
function rewrapRun(rows: readonly Row[], a: number, b: number, width: number): Row[] {
  const text = rows
    .slice(a, b + 1)
    .map((r, k) => (k === 0 ? r.text : r.text.trimStart()).replace(/ +$/, ''))
    .join(' ');
  return layout(text, width, false, null, a).rows;
}

/** Rewrap the paragraph that holds row `at`. */
export function rewrapParagraph(rows: readonly Row[], at: number, width: number): Row[] {
  const run = paragraphs(rows).find(([a, b]) => a <= at && at <= b);
  if (!run) return [...rows];
  const [a, b] = run;
  return [...rows.slice(0, a), ...rewrapRun(rows, a, b, width), ...rows.slice(b + 1)];
}

/** Rewrap every paragraph. */
export function rewrapAll(rows: readonly Row[], width: number): Row[] {
  let out: Row[] = [...rows];
  const runs = paragraphs(rows);
  for (let k = runs.length - 1; k >= 0; k -= 1) {
    const [a, b] = runs[k];
    out = [...out.slice(0, a), ...rewrapRun(out, a, b, width), ...out.slice(b + 1)];
  }
  return out;
}

/** A paste, its characters folded and each of its lines past `width`
 *  wrapped at its spaces, the breaks it brought kept. A word processor
 *  gives a paragraph one line, so this breaks exactly what needs it. */
export function pasted(
  text: string,
  width: number,
): { rows: Row[]; wrapped: number; folded: Folded[] } {
  const folding = fold(text.replace(/\r\n?/g, '\n'));
  const rows: Row[] = [];
  let wrapped = 0;
  for (const line of folding.text.split('\n')) {
    if (columns(line) <= width) {
      rows.push({ text: line, flows: false });
      continue;
    }
    wrapped += 1;
    rows.push(...layout(line, width, false, null, 0).rows);
  }
  return { rows, wrapped, folded: folding.folded };
}

// ── What the card marks ──────────────────────────────────────────────

/** A mark over part of a line. */
export interface Mark {
  from: number;
  to: number;
  kind: 'over' | 'soft' | 'quote' | 'dropped' | 'struck' | 'command';
}

/** The marks on `line`: what runs past `width`, in danger where a help
 *  sets the width and in warn where only Vosh does, a double quote, a
 *  character the game drops, a code inside the line a mortal's input
 *  loses, and a dot, @ or ! that opens it. */
export function marks(line: string, width: number, helpWidth: boolean, immortal: boolean): Mark[] {
  const out: Mark[] = [];
  if (columns(line) > width) {
    out.push({
      from: offsetOfColumn(line, width),
      to: line.replace(/ +$/, '').length,
      kind: helpWidth ? 'over' : 'soft',
    });
  }
  if (startsAsCommand(line)) out.push({ from: 0, to: 1, kind: 'command' });
  if (!immortal) {
    for (const [from, to] of innerCodes(line)) out.push({ from, to, kind: 'struck' });
  }
  for (let k = 0; k < line.length; k += 1) {
    const kind = marksCharacter(line[k]);
    if (kind) out.push({ from: k, to: k + 1, kind });
  }
  return out;
}

// ── The footer's count ───────────────────────────────────────────────

/** What the footer counts: lines with text, empty lines between them,
 *  lines past the width, and the room the text takes. */
export interface Count {
  lines: number;
  empty: number;
  past: number;
  bytes: number;
}

export function count(lines: readonly string[], width: number): Count {
  let last = lines.length - 1;
  while (last >= 0 && lines[last].trim().length === 0) last -= 1;
  const kept = lines.slice(0, last + 1);
  return {
    lines: kept.filter((l) => l.trim().length > 0).length,
    empty: kept.filter((l) => l.trim().length === 0).length,
    past: kept.filter((l) => columns(l) > width).length,
    bytes: storedBytes(kept),
  };
}

/** The game takes 26 identical lines in a row, other than one character
 *  and empty lines, as spam (comm.c:1516). The first row of such a run,
 *  or null. */
export function spamRun(lines: readonly string[]): number | null {
  let run = 1;
  for (let k = 1; k < lines.length; k += 1) {
    const same = lines[k] === lines[k - 1] && lines[k].trim().length > 1;
    run = same ? run + 1 : 1;
    if (run >= 26) return k - 25;
  }
  return null;
}
