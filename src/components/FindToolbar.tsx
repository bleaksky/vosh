import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import type { FindOptions } from './Terminal';

export interface FindToolbarHandle {
  /** Focus the query input. Called when the toolbar is already open
   *  and the user fires Cmd+F again so a second press re-selects the
   *  text for replacement instead of opening a stacked toolbar. */
  focus: () => void;
}

interface Props {
  /** Run a forward search. Returns true on hit so the toolbar can
   *  paint a "no match" indicator. */
  onFindNext: (query: string, options: FindOptions) => boolean;
  /** Run a backward search. */
  onFindPrevious: (query: string, options: FindOptions) => boolean;
  /** Close the toolbar. Hooked to the close button and Esc. The host
   *  is expected to clear search decorations on the terminal as part
   *  of closing. */
  onClose: () => void;
  /** Live match counts from the host. `index` is the 0-based active
   *  match position (or -1 when nothing is selected); `count` is the
   *  total number of matches across the scrollback. Displayed as
   *  "N / M" to the right of the search input. */
  results?: { index: number; count: number };
}

// Floating find toolbar that overlays the top-right of the terminal
// area. Drives xterm's SearchAddon through the Terminal handle so
// matches cover the full live scrollback, not just the visible
// viewport. Keyboard contract:
//   - Enter steps to the next match.
//   - Shift+Enter steps to the previous match.
//   - Esc closes the toolbar.
//
// State machine for the status indicator:
//   - `idle` while the input is empty (no badge rendered).
//   - `hit` when the last search call returned true.
//   - `miss` when it returned false.
// The indicator resets to `idle` on every keystroke so the user
// always sees the freshest result.
export const FindToolbar = forwardRef<FindToolbarHandle, Props>(function FindToolbar(
  { onFindNext, onFindPrevious, onClose, results }: Props,
  ref,
) {
  const [query, setQuery] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [regex, setRegex] = useState(false);
  const [status, setStatus] = useState<'idle' | 'hit' | 'miss'>('idle');
  // Track the active match position locally. The SearchAddon re-fires
  // (and resets) its own resultIndex on every terminal write while it
  // refreshes live highlights, so that index cannot be trusted while
  // MUD output is streaming. Its resultCount is reliable, so we step a
  // local index on each navigation and wrap it against that count.
  const [activeIndex, setActiveIndex] = useState(-1);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useImperativeHandle(
    ref,
    () => ({
      focus: () => {
        const el = inputRef.current;
        if (!el) return;
        el.focus();
        el.select();
      },
    }),
    [],
  );

  // Auto-focus on mount and select any existing query so the user can
  // type to replace immediately. setTimeout(0) defers the focus call
  // to the next macrotask, after any sibling components that mount in
  // the same render commit (notably the split-scrollback history
  // Terminal) finish their own setup. Without this, xterm's open()
  // call inside the history Terminal can land at the same tick and
  // race the focus call.
  useEffect(() => {
    const t = window.setTimeout(() => {
      const el = inputRef.current;
      if (!el) return;
      el.focus();
      el.select();
    }, 0);
    return () => window.clearTimeout(t);
  }, []);

  const options: FindOptions = { caseSensitive, wholeWord, regex };
  const count = results?.count ?? 0;

  const runNext = () => {
    if (query.length === 0) return;
    const hit = onFindNext(query, options);
    setStatus(hit ? 'hit' : 'miss');
    if (hit) setActiveIndex((i) => (count > 0 ? (i + 1 + count) % count : 0));
  };
  const runPrevious = () => {
    if (query.length === 0) return;
    const hit = onFindPrevious(query, options);
    setStatus(hit ? 'hit' : 'miss');
    if (hit) setActiveIndex((i) => (count > 0 ? (i - 1 + count) % count : 0));
  };

  // Any change to the query or match options restarts the search, so
  // the next navigation should land on the first hit again.
  const resetSearch = () => {
    setStatus('idle');
    setActiveIndex(-1);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) runPrevious();
      else runNext();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div
      className="find-toolbar"
      role="search"
      aria-label="search scrollback"
      // Stop wheel from leaking into the terminal area so scrolling
      // over the toolbar does not open the split scrollback view.
      onWheel={(e) => e.stopPropagation()}
      // Same for mouseup so a click on the toolbar does not also
      // re-focus the command input.
      onMouseUp={(e) => e.stopPropagation()}
    >
      <input
        ref={inputRef}
        type="search"
        className="find-toolbar-input"
        placeholder="find in scrollback"
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          resetSearch();
        }}
        onKeyDown={handleKeyDown}
        aria-label="search term"
      />
      <span className={`find-toolbar-status find-toolbar-status-${status}`} aria-live="polite">
        {status === 'miss'
          ? 'no match'
          : count > 0
            ? `${Math.max(0, activeIndex) + 1} / ${count}`
            : status === 'hit'
              ? 'found'
              : ''}
      </span>
      <button
        type="button"
        className="find-toolbar-btn"
        onClick={runPrevious}
        disabled={query.length === 0}
        aria-label="previous match"
        title="previous (Shift+Enter)"
      >
        ↑
      </button>
      <button
        type="button"
        className="find-toolbar-btn"
        onClick={runNext}
        disabled={query.length === 0}
        aria-label="next match"
        title="next (Enter)"
      >
        ↓
      </button>
      <label
        className={`find-toolbar-toggle${caseSensitive ? ' is-on' : ''}`}
        title="match case (case sensitive)"
      >
        <input
          type="checkbox"
          checked={caseSensitive}
          onChange={(e) => {
            setCaseSensitive(e.target.checked);
            resetSearch();
          }}
        />
        case
      </label>
      <label
        className={`find-toolbar-toggle${wholeWord ? ' is-on' : ''}`}
        title="match whole word only"
      >
        <input
          type="checkbox"
          checked={wholeWord}
          onChange={(e) => {
            setWholeWord(e.target.checked);
            resetSearch();
          }}
        />
        word
      </label>
      <label
        className={`find-toolbar-toggle${regex ? ' is-on' : ''}`}
        title="treat the query as a regular expression"
      >
        <input
          type="checkbox"
          checked={regex}
          onChange={(e) => {
            setRegex(e.target.checked);
            resetSearch();
          }}
        />
        regex
      </label>
      <button
        type="button"
        className="find-toolbar-close"
        onClick={onClose}
        aria-label="close find toolbar"
        title="close (Esc)"
      >
        ×
      </button>
    </div>
  );
});
