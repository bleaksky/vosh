import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  fieldHtml,
  insertAt,
  keptCaret,
  oneLine,
  pieceAtCaret,
  TEXT_HELP,
  TOKEN_ROWS,
} from './textEdit';
import type { PromptToken } from '../ipc/promptDesign';
import { Button, PlusIcon } from '../ui';

// Edit as text: your design byte for byte in the terminal's face, each
// token colored by what it is, wrapping only between tokens. The token
// under the caret carries the selection token, and the part it draws
// carries the accent tint and ring on your prompt. Every change saves
// as you type and redraws your prompt. The token rows add a token at
// the caret, and Insert value… picks a value and adds its token there.

interface PromptTextProps {
  template: string;
  /** The tokens of `describedFor`, which trails the field while you type. */
  tokens: readonly PromptToken[];
  describedFor: string;
  onChange: (template: string) => void;
  /** The part the token at the caret draws, for the mark on your prompt. */
  onCaretPiece: (piece: number | null) => void;
  /** Open the picker. What it picks comes back through `insertRef`. */
  onInsertValue: () => void;
  /** The card hands the field the token a picked value writes. */
  insertRef: { current: ((token: string) => void) | null };
  /** Where the caret is, which the card keeps while Insert value…
   *  replaces the field, so the value goes in there. */
  caretRef: { current: { start: number; end: number } | null };
  /** Above 0 while Edit prompt as text… asks for the field, so what you
   *  type goes to your design. The field takes focus with the caret where
   *  it was, then calls `onFocusTaken`. */
  focusRequest?: number;
  onFocusTaken?: () => void;
  /** What a reader hears the field named. */
  fieldLabel?: string | undefined;
}

/** The caret's offset in `el`'s text, or null when the caret is not in it. */
function caretOf(el: HTMLElement): { start: number; end: number } | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!el.contains(range.startContainer) || !el.contains(range.endContainer)) return null;
  const before = document.createRange();
  before.selectNodeContents(el);
  before.setEnd(range.startContainer, range.startOffset);
  const start = before.toString().length;
  return { start, end: start + range.toString().length };
}

/** Put the caret at `offset` in `el`'s text. */
function placeCaret(el: HTMLElement, offset: number): void {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let left = offset;
  let node = walker.nextNode();
  while (node) {
    const length = node.textContent?.length ?? 0;
    if (left <= length) {
      const range = document.createRange();
      range.setStart(node, left);
      range.collapse(true);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      return;
    }
    left -= length;
    node = walker.nextNode();
  }
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

export function PromptText({
  template,
  tokens,
  describedFor,
  onChange,
  onCaretPiece,
  onInsertValue,
  insertRef,
  caretRef,
  focusRequest = 0,
  onFocusTaken,
  fieldLabel = 'Prompt template',
}: PromptTextProps) {
  const fieldRef = useRef<HTMLDivElement | null>(null);
  const [text, setText] = useState(template);
  const sent = useRef(template);
  const caret = useRef<{ start: number; end: number }>(keptCaret(template, caretRef.current));
  // The card keeps every move of the caret.
  const moveCaret = (next: { start: number; end: number }) => {
    caret.current = next;
    caretRef.current = next;
  };
  const [marked, setMarked] = useState<number | null>(null);

  // A change from elsewhere, an undo among them, replaces the text.
  useEffect(() => {
    if (template !== sent.current) {
      sent.current = template;
      setText(template);
      const end = Math.min(caret.current.end, template.length);
      caret.current = { start: end, end };
      caretRef.current = caret.current;
    }
  }, [template, caretRef]);

  const follow = (next: { start: number; end: number }) => {
    moveCaret(next);
    if (describedFor === text) {
      const piece = pieceAtCaret(tokens, next.end);
      setMarked(piece);
      onCaretPiece(piece);
    }
  };

  // The tokens catch up with the text, so the mark follows the caret.
  useEffect(() => {
    if (describedFor !== text) return;
    const piece = pieceAtCaret(tokens, caret.current.end);
    setMarked(piece);
    onCaretPiece(piece);
    // onCaretPiece is the card's setter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tokens, describedFor, text]);

  const html =
    describedFor === text
      ? fieldHtml(text, tokens, marked)
      : text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

  // The markup changes under the caret, so it goes back where it was.
  useLayoutEffect(() => {
    const el = fieldRef.current;
    if (!el) return;
    if (el.innerHTML !== html) el.innerHTML = html;
    if (document.activeElement === el) placeCaret(el, caret.current.end);
  }, [html]);

  // Edit prompt as text… puts you in the field.
  const focusTaken = useRef(onFocusTaken);
  focusTaken.current = onFocusTaken;
  useEffect(() => {
    const el = fieldRef.current;
    if (focusRequest <= 0 || !el) return;
    el.focus({ preventScroll: true });
    placeCaret(el, caret.current.end);
    focusTaken.current?.();
  }, [focusRequest]);

  const commit = (next: string, at: number) => {
    moveCaret({ start: at, end: at });
    setText(next);
    if (next !== sent.current) {
      sent.current = next;
      onChange(next);
    }
  };

  const insert = (token: string) => {
    const { start, end } = caret.current;
    const done = insertAt(text, start, end, token);
    commit(done.text, done.caret);
    fieldRef.current?.focus({ preventScroll: true });
  };
  insertRef.current = insert;

  return (
    <div className="pc-body pc-text">
      <span id="pc-template-label" className="st-visually-hidden">
        {fieldLabel}
      </span>
      <div
        ref={fieldRef}
        className="pc-text-field"
        contentEditable
        suppressContentEditableWarning
        spellCheck={false}
        role="textbox"
        aria-multiline="true"
        aria-labelledby="pc-template-label"
        onInput={(e) => {
          const el = e.currentTarget;
          const at = caretOf(el)?.end ?? el.textContent?.length ?? 0;
          const raw = el.textContent ?? '';
          const next = oneLine(raw);
          commit(next, Math.min(at, next.length));
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            insert('%nl');
          }
          // Left and Right move the caret here, not between parts.
          if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') e.stopPropagation();
        }}
        onKeyUp={(e) => {
          const at = caretOf(e.currentTarget);
          if (at) follow(at);
        }}
        onMouseUp={(e) => {
          const at = caretOf(e.currentTarget);
          if (at) follow(at);
        }}
        onPaste={(e) => {
          e.preventDefault();
          insert(oneLine(e.clipboardData.getData('text/plain')));
        }}
      />
      <p className="pc-text-help">{TEXT_HELP}</p>
      <div className="pc-text-rows">
        {TOKEN_ROWS.map((row) => (
          <div key={row.label} className="pc-text-row">
            <span className="pc-text-row-label">{row.label}</span>
            <span className="pc-text-chips">
              {row.tokens.map((token) => (
                <button
                  key={token}
                  type="button"
                  className="pc-chip"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => insert(token)}
                >
                  {token}
                </button>
              ))}
            </span>
          </div>
        ))}
      </div>
      <div className="pc-piece-actions">
        <Button icon={<PlusIcon />} onClick={onInsertValue}>
          Insert value…
        </Button>
      </div>
    </div>
  );
}
