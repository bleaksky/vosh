import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import CodeMirror, {
  Annotation,
  Compartment,
  Decoration,
  EditorState,
  EditorView,
  GutterMarker,
  RangeSetBuilder,
  StateEffect,
  StateField,
  ViewPlugin,
  WidgetType,
  gutter,
  type DecorationSet,
  type Extension,
  type Text,
  type Transaction,
  type ViewUpdate,
} from '@uiw/react-codemirror';
import { codeSlot } from './gameCodes';
import { flow, leadingCode, marks, pasted, type Folded, type Row } from './text';

// The text box of the writing card (Description Editor board 1). Its
// lines are the lines the game gets: a two digit gutter, the guide at
// the right edge of the width the card keeps, and the marks the card
// draws on what the game would change. A paragraph flows as you type
// and a break you make with Return stays (text.ts). While Vosh sends,
// the gutter checks off each line the game took.

/** What a send looks like in the box: the lines the game took, and the
 *  line on its way. */
export interface Sending {
  sent: number;
  current: number | null;
}

/** What a paste changed, for the footer. */
export interface PasteNote {
  wrapped: number;
  folded: Folded[];
  /** The word a single fold sat in, which the footer names. */
  word: string | null;
}

/** The text from outside, a read or a rewrap, which replaces what the
 *  box holds. `revision` moves with each one. */
export interface BoxText {
  rows: Row[];
  revision: number;
  caret?: { row: number; col: number } | null;
}

export interface WritingBoxProps {
  text: BoxText;
  width: number;
  helpWidth: boolean;
  immortal: boolean;
  spellcheck: boolean;
  readOnly: boolean;
  sending: Sending | null;
  /** The first line the game's editor would refuse. */
  cut: number | null;
  /** The terminal's 16 colors, for a line a code colors. */
  palette: readonly string[];
  label: string;
  /** The rows the box shows before it scrolls. */
  rows: number;
  minRows: number;
  onChange: (rows: Row[]) => void;
  onCaret: (row: number) => void;
  onPaste: (note: PasteNote) => void;
  /** What the box says in its text's place while the text is empty,
   *  and the game's line under it. */
  empty?: { says: string; line: string | null } | null;
}

/** Text from outside the box, which no flow touches. */
const Outside = Annotation.define<boolean>();

/** A paste, with whether each break it brought flows. */
const Pasted = Annotation.define<{ from: number; flows: boolean[] }>();

const setFlows = StateEffect.define<boolean[]>();
const setSending = StateEffect.define<Sending | null>();

/** Whether each line flows into the next one. */
function flowsField(initial: boolean[]) {
  return StateField.define<boolean[]>({
    create: () => initial,
    update(value, tr) {
      for (const e of tr.effects) if (e.is(setFlows)) return e.value;
      return tr.docChanged ? mapFlows(value, tr) : value;
    },
  });
}

/** The flows of the lines after `tr`. A break that survives keeps its
 *  flag, and a new one, a Return you typed, does not flow. */
function mapFlows(before: boolean[], tr: Transaction): boolean[] {
  const doc = tr.newDoc;
  const out: boolean[] = Array.from({ length: doc.lines }, () => false);
  const old = tr.startState.doc;
  for (let n = 1; n < old.lines; n += 1) {
    if (!before[n - 1]) continue;
    const at = old.line(n).to;
    if (tr.changes.touchesRange(at, at + 1)) continue;
    const mapped = tr.changes.mapPos(at, 1);
    out[doc.lineAt(mapped).number - 1] = true;
  }
  const paste = tr.annotation(Pasted);
  if (paste) {
    let line = doc.lineAt(paste.from).number;
    for (const flows of paste.flows) {
      out[line - 1] = flows;
      line += 1;
    }
  }
  return out;
}

function rowsOf(doc: Text, flows: boolean[]): Row[] {
  const rows: Row[] = [];
  for (let n = 1; n <= doc.lines; n += 1) {
    rows.push({ text: doc.line(n).text, flows: flows[n - 1] ?? false });
  }
  return rows;
}

/** Flow the paragraph under the caret after you type or delete, in the
 *  same step, so undo takes both back. */
function flowAfter(width: () => number, field: StateField<boolean[]>): Extension {
  return EditorState.transactionFilter.of((tr) => {
    if (!tr.docChanged || tr.annotation(Outside)) return tr;
    const flows = mapFlows(tr.startState.field(field), tr);
    const typing = (tr.isUserEvent('input') || tr.isUserEvent('delete')) && !tr.annotation(Pasted);
    if (!typing) return [tr, { effects: setFlows.of(flows), sequential: true }];
    const doc = tr.newDoc;
    const head = tr.newSelection.main.head;
    const line = doc.lineAt(head);
    const rows = rowsOf(doc, flows);
    const caret = { row: line.number - 1, col: head - line.from };
    const out = flow(rows, caret, width());
    const changed =
      out.rows.length !== rows.length || out.rows.some((r, k) => r.text !== rows[k].text);
    if (!changed)
      return [tr, { effects: setFlows.of(out.rows.map((r) => r.flows)), sequential: true }];
    // Replace only the rows that moved.
    let a = 0;
    while (a < rows.length && a < out.rows.length && rows[a].text === out.rows[a].text) a += 1;
    let b = rows.length - 1;
    let c = out.rows.length - 1;
    while (b > a && c > a && rows[b].text === out.rows[c].text) {
      b -= 1;
      c -= 1;
    }
    const from = doc.line(a + 1).from;
    const to = doc.line(b + 1).to;
    const insert = out.rows
      .slice(a, c + 1)
      .map((r) => r.text)
      .join('\n');
    let anchor = from;
    for (let k = a; k < out.caret.row; k += 1) anchor += out.rows[k].text.length + 1;
    anchor += out.caret.col;
    return [
      tr,
      {
        changes: { from, to, insert },
        selection: { anchor },
        effects: setFlows.of(out.rows.map((r) => r.flows)),
        sequential: true,
      },
    ];
  });
}

/** The 6 px dot a code at a line's start draws as, at the gutter's
 *  left, in its color, the code itself out of the way. */
class CodeDot extends WidgetType {
  constructor(
    readonly code: string,
    readonly color: string,
  ) {
    super();
  }
  override eq(other: CodeDot): boolean {
    return other.code === this.code && other.color === this.color;
  }
  toDOM(): HTMLElement {
    const dot = document.createElement('span');
    dot.className = 'wr-code';
    dot.title = `\`${this.code}`;
    dot.style.setProperty('--wr-code', this.color);
    return dot;
  }
}

/** A code inside a line an immortal keeps, a dot of no width. */
class InnerCode extends WidgetType {
  constructor(
    readonly code: string,
    readonly color: string,
  ) {
    super();
  }
  override eq(other: InnerCode): boolean {
    return other.code === this.code && other.color === this.color;
  }
  toDOM(): HTMLElement {
    const tick = document.createElement('span');
    tick.className = 'wr-code-inner';
    tick.title = `\`${this.code}`;
    tick.style.setProperty('--wr-code', this.color);
    return tick;
  }
}

interface Look {
  width: number;
  helpWidth: boolean;
  immortal: boolean;
  palette: readonly string[];
  cut: number | null;
  sending: Sending | null;
}

/** The color a code draws its line in, and whether bold. */
function colorOf(
  code: string,
  palette: readonly string[],
): { color: string; bold: boolean } | null {
  const slot = codeSlot(code);
  if (!slot) return null;
  return { color: palette[slot.color] ?? 'inherit', bold: slot.bold };
}

/** The marks of every line. */
function decorate(doc: Text, look: Look): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (let n = 1; n <= doc.lines; n += 1) {
    const line = doc.line(n);
    const text = line.text;
    const k = n - 1;
    const classes: string[] = [];
    if (look.cut !== null && k >= look.cut) classes.push('is-cut');
    if (look.sending) {
      if (look.sending.current === k) classes.push('is-current');
      else if (k >= look.sending.sent) classes.push('is-waiting');
    }
    if (classes.length > 0)
      builder.add(line.from, line.from, Decoration.line({ class: classes.join(' ') }));
    type Piece = { from: number; to: number; deco: Decoration };
    const pieces: Piece[] = [];
    const code = leadingCode(text);
    const drawn = code ? colorOf(code, look.palette) : null;
    if (code) {
      pieces.push({
        from: line.from,
        to: line.from + 2,
        deco: Decoration.replace({ widget: new CodeDot(code, drawn?.color ?? 'transparent') }),
      });
      if (drawn && text.length > 2) {
        pieces.push({
          from: line.from + 2,
          to: line.to,
          deco: Decoration.mark({
            attributes: { style: `color:${drawn.color}${drawn.bold ? ';font-weight:700' : ''}` },
          }),
        });
      }
    }
    for (const m of marks(text, look.width, look.helpWidth, look.immortal)) {
      if (m.kind === 'struck' || m.from >= m.to) {
        if (m.kind === 'struck')
          pieces.push({ from: line.from + m.from, to: line.from + m.to, deco: STRUCK });
        continue;
      }
      pieces.push({ from: line.from + m.from, to: line.from + m.to, deco: MARKS[m.kind] });
    }
    if (look.immortal) {
      for (let i = code ? 2 : 0; i < text.length - 1; i += 1) {
        if (text[i] !== '`') continue;
        const inner = colorOf(text[i + 1], look.palette);
        pieces.push({
          from: line.from + i,
          to: line.from + i + 2,
          deco: Decoration.replace({
            widget: new InnerCode(text[i + 1], inner?.color ?? 'currentColor'),
          }),
        });
        i += 1;
      }
    }
    pieces.sort((x, y) => x.from - y.from || (x.deco.startSide ?? 0) - (y.deco.startSide ?? 0));
    for (const p of pieces) builder.add(p.from, p.to, p.deco);
  }
  return builder.finish();
}

const MARKS = {
  over: Decoration.mark({ class: 'wr-over' }),
  soft: Decoration.mark({ class: 'wr-soft' }),
  quote: Decoration.mark({ class: 'wr-warn' }),
  dropped: Decoration.mark({ class: 'wr-warn' }),
  command: Decoration.mark({ class: 'wr-warn' }),
  struck: Decoration.mark({ class: 'wr-struck' }),
};
const STRUCK = MARKS.struck;

/** A number in the gutter, or a check for a line the game took. */
class Num extends GutterMarker {
  constructor(
    readonly n: number,
    readonly state: '' | 'over' | 'soft' | 'cut' | 'current' | 'sent',
  ) {
    super();
  }
  override eq(other: Num): boolean {
    return other.n === this.n && other.state === this.state;
  }
  override toDOM(): Node {
    const el = document.createElement('span');
    el.className = `wr-num${this.state ? ` is-${this.state}` : ''}`;
    if (this.state === 'sent') {
      el.innerHTML =
        '<svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
    } else {
      el.textContent = String(this.n);
    }
    return el;
  }
}

function numbers(look: () => Look): Extension {
  return gutter({
    class: 'wr-gutter',
    lineMarker(view, line) {
      const n = view.state.doc.lineAt(line.from).number;
      const k = n - 1;
      const l = look();
      const text = view.state.doc.line(n).text;
      let state: Num['state'] = '';
      if (l.sending && k < l.sending.sent) state = 'sent';
      else if (l.sending && l.sending.current === k) state = 'current';
      else if (l.cut !== null && k >= l.cut) state = 'cut';
      else {
        const over = marks(text, l.width, l.helpWidth, l.immortal).find(
          (m) => m.kind === 'over' || m.kind === 'soft',
        );
        if (over) state = over.kind === 'over' ? 'over' : 'soft';
      }
      return new Num(n, state);
    },
    lineMarkerChange: (update) =>
      update.docChanged || update.transactions.some((t) => t.effects.some((e) => e.is(setSending))),
  });
}

export function WritingBox({
  text,
  width,
  helpWidth,
  immortal,
  spellcheck,
  readOnly,
  sending,
  cut,
  palette,
  label,
  rows,
  minRows,
  onChange,
  onCaret,
  onPaste,
  empty = null,
}: WritingBoxProps) {
  const [view, setView] = useState<EditorView | null>(null);
  // The look the decorations and the gutter read, through refs, so the
  // extensions build once.
  const look = useRef<Look>({ width, helpWidth, immortal, palette, cut, sending });
  look.current = { width, helpWidth, immortal, palette, cut, sending };
  const callbacks = useRef({ onChange, onCaret, onPaste });
  callbacks.current = { onChange, onCaret, onPaste };
  // What the box starts with, and the pieces its extensions keep, made
  // once as it mounts.
  const [start] = useState(() => ({
    doc: text.rows.map((r) => r.text).join('\n'),
    field: flowsField(text.rows.map((r) => r.flows)),
    spell: new Compartment(),
    editable: new Compartment(),
    spellcheck,
    readOnly,
  }));
  const { field, spell, editable } = start;

  const extensions = useMemo<Extension[]>(() => {
    const marksPlugin = ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;
        constructor(v: EditorView) {
          this.decorations = decorate(v.state.doc, look.current);
        }
        update(u: ViewUpdate) {
          const moved = u.transactions.some((t) => t.effects.some((e) => e.is(setSending)));
          if (u.docChanged || moved || u.viewportChanged)
            this.decorations = decorate(u.state.doc, look.current);
        }
      },
      { decorations: (p) => p.decorations },
    );
    return [
      field,
      flowAfter(() => look.current.width, field),
      numbers(() => look.current),
      marksPlugin,
      spell.of(contentAttributes(start.spellcheck)),
      editable.of([
        EditorView.editable.of(!start.readOnly),
        EditorState.readOnly.of(start.readOnly),
      ]),
      EditorView.updateListener.of((u) => {
        // Text from outside is the card's own already.
        const outside = u.transactions.some((t) => t.annotation(Outside));
        if (u.docChanged && !outside) {
          const flows = u.state.field(field);
          callbacks.current.onChange(rowsOf(u.state.doc, flows));
        }
        if (u.docChanged || u.selectionSet) {
          callbacks.current.onCaret(u.state.doc.lineAt(u.state.selection.main.head).number - 1);
        }
      }),
      EditorView.domEventHandlers({
        paste(event, v) {
          const clip = event.clipboardData?.getData('text/plain');
          if (clip === undefined) return false;
          event.preventDefault();
          const p = pasted(clip, look.current.width);
          const insert = p.rows.map((r) => r.text).join('\n');
          const { from, to } = v.state.selection.main;
          v.dispatch({
            changes: { from, to, insert },
            selection: { anchor: from + insert.length },
            userEvent: 'input.paste',
            annotations: Pasted.of({ from, flows: p.rows.map((r) => r.flows) }),
            scrollIntoView: true,
          });
          if (p.wrapped > 0 || p.folded.length > 0) {
            const single = p.folded.length === 1 && p.folded[0].count === 1;
            callbacks.current.onPaste({
              wrapped: p.wrapped,
              folded: p.folded,
              word: single ? foldedWord(clip) : null,
            });
          }
          return true;
        },
      }),
    ];
    // The look reads through refs, and the compartments carry the rest.
  }, [start, field, spell, editable]);

  useEffect(() => {
    view?.dispatch({ effects: spell.reconfigure(contentAttributes(spellcheck)) });
  }, [view, spell, spellcheck]);

  useEffect(() => {
    view?.dispatch({
      effects: editable.reconfigure([
        EditorView.editable.of(!readOnly),
        EditorState.readOnly.of(readOnly),
      ]),
    });
  }, [view, editable, readOnly]);

  const sendingAt = sending ? `${sending.sent}:${sending.current ?? ''}` : null;
  useEffect(() => {
    if (!view) return;
    const now = look.current.sending;
    const effects: StateEffect<unknown>[] = [setSending.of(now)];
    // The line on its way stays in view.
    if (now?.current !== null && now?.current !== undefined && now.current < view.state.doc.lines) {
      effects.push(
        EditorView.scrollIntoView(view.state.doc.line(now.current + 1).from, { y: 'nearest' }),
      );
    }
    view.dispatch({ effects });
  }, [view, sendingAt]);

  // Text from outside replaces what the box holds.
  const shown = useRef(text.revision);
  useEffect(() => {
    if (!view || shown.current === text.revision) return;
    shown.current = text.revision;
    const insert = text.rows.map((r) => r.text).join('\n');
    let anchor = insert.length;
    if (text.caret) {
      anchor = 0;
      for (let k = 0; k < text.caret.row && k < text.rows.length; k += 1)
        anchor += text.rows[k].text.length + 1;
      anchor = Math.min(insert.length, anchor + text.caret.col);
    }
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert },
      selection: { anchor },
      effects: [setFlows.of(text.rows.map((r) => r.flows)), setSending.of(look.current.sending)],
      annotations: Outside.of(true),
    });
  }, [view, text]);

  const style = {
    '--wr-rows': rows,
    '--wr-min-rows': minRows,
  } as CSSProperties;

  return (
    <div className="wr-box" style={style} data-width={width}>
      {empty && (
        <p className="wr-empty">
          {empty.says}
          {empty.line && <code>{empty.line}</code>}
        </p>
      )}
      <CodeMirror
        value={start.doc}
        theme="none"
        extensions={extensions}
        onCreateEditor={setView}
        basicSetup={{
          lineNumbers: false,
          foldGutter: false,
          highlightActiveLine: false,
          highlightActiveLineGutter: false,
          highlightSelectionMatches: false,
          bracketMatching: false,
          closeBrackets: false,
          autocompletion: false,
          searchKeymap: false,
          indentOnInput: false,
          dropCursor: true,
          allowMultipleSelections: false,
          history: true,
          historyKeymap: true,
          defaultKeymap: true,
        }}
        aria-label={label}
      />
    </div>
  );
}

/** The attributes of the text: the card's own spell check, and no
 *  autocorrect, capitals or text predictions, as the command line keeps
 *  them off (Description Editor Q9). */
function contentAttributes(spellcheck: boolean): Extension {
  return EditorView.contentAttributes.of({
    spellcheck: spellcheck ? 'true' : 'false',
    autocorrect: 'off',
    autocapitalize: 'off',
    writingsuggestions: 'false',
    'aria-multiline': 'true',
  });
}

/** The word around the one character a paste folded. */
function foldedWord(text: string): string | null {
  const m =
    /[^\s]*[\u2018\u2019\u201a\u201b\u2032\u201c\u201d\u201e\u201f\u2033\u2014\u2013\u2012\u2212\u2026][^\s]*/.exec(
      text,
    );
  return m ? m[0].replace(/[\u2018\u2019\u201a\u201b\u2032]/g, '\u2019') : null;
}
