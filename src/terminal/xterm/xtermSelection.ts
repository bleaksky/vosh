import type { IDisposable, Terminal } from '@xterm/xterm';
import { noteReader } from '../readerBusy';

/** What the selection watch leaves for the pane to undo as it goes. */
export interface SelectionWatch {
  scroll: IDisposable;
  readerSelection: IDisposable;
  readerBack: IDisposable | null;
  /** Takes the copy key listener off the window. */
  removeCopyKey: () => void;
  /** The reader part the pane's selection notes go to. */
  selectionPart: 'historySelection' | 'liveSelection';
}

/** Watch the pane's selection and the window's copy key. `quiet` is the
 *  history pane, read once as the pane mounts, and `shown` whether the
 *  pane shows now. */
export function watchSelection(
  term: Terminal,
  quiet: boolean,
  shown: { readonly current: boolean },
  session: number,
): SelectionWatch {
  // Auto-clear selection when it scrolls off the viewport.
  // xterm's canvas renderer paints the selection overlay at the
  // selection's current row in the viewport, but the underlying
  // selection POSITION stays anchored to its original buffer rows
  // even when new data scrolls the buffer up — the paint then
  // happens at a stale screen position ("ghost"). Subscribing to
  // onScroll lets us notice when the selection range has fallen
  // outside [viewportY, viewportY + rows] and clear it before the
  // ghost can render.
  const onScrollClearStaleSelection = () => {
    const sel = term.getSelectionPosition();
    if (!sel) return;
    const top = term.buffer.active.viewportY;
    const bottom = top + term.rows - 1;
    const offTop = sel.end.y < top;
    const offBottom = sel.start.y > bottom;
    if (offTop || offBottom) {
      term.clearSelection();
    }
  };
  const scrollDisposable = term.onScroll(onScrollClearStaleSelection);

  // A clock piece in your design leaves your prompt as it is while you
  // select text here, or while the live pane is off its newest rows.
  const selectionPart = quiet ? 'historySelection' : 'liveSelection';
  const readerSelection = term.onSelectionChange(() =>
    noteReader(selectionPart, term.hasSelection(), session),
  );
  const readerBack = quiet
    ? null
    : term.onScroll(() =>
        noteReader('liveBack', term.buffer.active.viewportY !== term.buffer.active.baseY, session),
      );

  // Ctrl/Cmd + C or X copies the xterm selection. The keystroke
  // almost always lands while focus is in the Input box (the user
  // drag-selects xterm output, then hits the shortcut without
  // clicking back into the terminal), so a keydown listener
  // attached to xterm alone never fires. Listen at the window
  // instead, and defer to the focused element's native copy/cut
  // when it actually has its own selection. A hidden pane keeps its
  // selection for when it shows, and copies nothing meanwhile.
  const onCopyKey = (event: KeyboardEvent) => {
    if (!shown.current) return;
    const key = event.key.toLowerCase();
    if (key !== 'c' && key !== 'x') return;
    // Accept any combination of Ctrl or Cmd (without Alt), with or
    // without Shift. Plain Ctrl+C is the convention most MUD clients
    // use; the older Ctrl+Shift+C variant still works.
    const primary = event.ctrlKey || event.metaKey;
    if (!primary || event.altKey) return;
    const selection = term.getSelection();
    if (!selection) return;
    const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
    const activeHasSelection =
      active && 'selectionStart' in active && active.selectionStart !== active.selectionEnd;
    const domSelection = window.getSelection();
    const domHasSelection = domSelection !== null && domSelection.toString().length > 0;
    if (activeHasSelection || domHasSelection) return;
    void navigator.clipboard.writeText(selection).catch(() => {
      /* clipboard may be unavailable in some webviews */
    });
    event.preventDefault();
    event.stopPropagation();
    // Return the caret to the command line so the user keeps typing
    // instead of leaving focus stranded on the terminal.
    window.dispatchEvent(new Event('vosh:focus-input'));
  };
  window.addEventListener('keydown', onCopyKey, true);

  return {
    scroll: scrollDisposable,
    readerSelection,
    readerBack,
    removeCopyKey: () => window.removeEventListener('keydown', onCopyKey, true),
    selectionPart,
  };
}
