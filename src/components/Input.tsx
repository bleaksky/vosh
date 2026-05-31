import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type ClipboardEvent,
  type KeyboardEvent,
} from 'react';
import {
  getTarget,
  getUiConfig,
  listMacros,
  onGmcpPackage,
  onInputMode,
  onTarget,
  sendInput,
  subscribeMacrosChanged,
  type Macro,
  type QuickKey,
} from '../lib/session';
import { canonicalKeyFromEvent } from '../lib/macroKeys';
import { recentNames } from '../lib/recentNames';
import { listen } from '@tauri-apps/api/event';

export interface InputHandle {
  focus: () => void;
}

interface Props {
  enabled: boolean;
  onError?: (message: string) => void;
  onLocalEcho?: (text: string) => void;
  /** Scroll the terminal scrollback by N pages. Called from
   *  PageUp/PageDown handling (which on macOS is Fn+Up/Fn+Down). */
  onScrollTerminal?: (pages: number) => void;
  /** Close the split-scrollback view. Fires on Esc; the host decides
   *  whether anything is currently open. */
  onExitSplit?: () => void;
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  { enabled, onError, onLocalEcho, onScrollTerminal, onExitSplit }: Props,
  ref,
) {
  const [value, setValue] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  const [passwordMode, setPasswordMode] = useState(false);
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (enabled) inputRef.current?.focus();
  }, [enabled]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    onInputMode((payload) => {
      setPasswordMode(payload.password);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Room characters from Room.Chars GMCP. Used as a noun source for
  // Tab completion so the user can complete combat target names
  // without typing the whole word.
  const roomCharsRef = useRef<string[]>([]);
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    onGmcpPackage<unknown>('Room.Chars', (data) => {
      if (!Array.isArray(data)) {
        roomCharsRef.current = [];
        return;
      }
      const names: string[] = [];
      for (const entry of data) {
        if (entry && typeof entry === 'object') {
          const name = (entry as { name?: unknown }).name;
          if (typeof name === 'string' && name.length > 0) {
            names.push(name);
          }
        }
      }
      roomCharsRef.current = names;
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Tab-completion cycling state. When the user presses Tab we
  // resolve the word being typed, build a candidate list, and
  // remember the cycle so consecutive Tab presses walk through the
  // matches. Any input change other than Tab resets this so the next
  // Tab starts a fresh search.
  const tabStateRef = useRef<{
    wordStart: number;
    matches: string[];
    idx: number;
    suffixOffset: number;
  } | null>(null);

  // Track configured quick-keys so we can skip the local echo when
  // the user types one. The backend echoes the expansion (`bash
  // blah`) via session://output, so the shortcut itself never lands
  // in xterm — only the resolved command does.
  const quickKeysRef = useRef<QuickKey[]>([]);
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    getTarget()
      .then((snap) => {
        if (!cancelled) quickKeysRef.current = snap.quick_keys;
      })
      .catch(() => {});
    onTarget((payload) => {
      quickKeysRef.current = payload.quick_keys;
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Keep-last-command preference. When true, after submitting a
  // line the input retains the value and selects the text so
  // pressing Enter resends. Read once on mount, refreshed via the
  // vosh://keep-last-changed event the settings save fires.
  const keepLastRef = useRef<boolean>(false);
  // Paste-line delay (ms). Same load + subscribe pattern as keepLast
  // so the indicator/pacing picks up Settings edits without a relaunch.
  const pasteDelayRef = useRef<number>(500);
  useEffect(() => {
    let cancelled = false;
    let unlistenKeep: (() => void) | undefined;
    let unlistenPaste: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (cancelled) return;
        keepLastRef.current = cfg.keep_last_command;
        pasteDelayRef.current = cfg.paste_line_delay_ms;
      })
      .catch(() => {});
    listen<boolean>('vosh://keep-last-changed', (event) => {
      keepLastRef.current = Boolean(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenKeep = fn;
    });
    listen<number>('vosh://paste-line-delay-changed', (event) => {
      const n = Number(event.payload);
      if (Number.isFinite(n) && n >= 0) {
        pasteDelayRef.current = Math.min(10_000, Math.floor(n));
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenPaste = fn;
    });
    return () => {
      cancelled = true;
      unlistenKeep?.();
      unlistenPaste?.();
    };
  }, []);

  // Keyboard macro bindings — keyed by canonical key string
  // ("F1", "Ctrl+N", "Numpad7"). Seeded from the backend and
  // refreshed on every macros-changed broadcast.
  const macroMapRef = useRef<Map<string, string>>(new Map());
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    const apply = (list: Macro[]) => {
      const m = new Map<string, string>();
      for (const entry of list) {
        if (entry.key && entry.command) m.set(entry.key, entry.command);
      }
      macroMapRef.current = m;
    };
    listMacros()
      .then((list) => {
        if (!cancelled) apply(list);
      })
      .catch(() => {});
    subscribeMacrosChanged((list) => {
      if (!cancelled) apply(list);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      focus: () => inputRef.current?.focus(),
    }),
    [],
  );

  const matchingIndices = (prefix: string | null): number[] => {
    if (prefix === null || prefix === '') {
      return history.map((_, i) => i);
    }
    return history.flatMap((line, i) => (line.startsWith(prefix) ? [i] : []));
  };

  const startSearchIfNeeded = (): number[] => {
    if (searchPrefix === null) {
      const prefix = value;
      setSearchPrefix(prefix);
      return matchingIndices(prefix);
    }
    return matchingIndices(searchPrefix);
  };

  const handleChange = (next: string) => {
    setValue(next);
    // Any direct edit cancels the active prefix search so the next Up uses
    // the current input as the new prefix.
    if (searchPrefix !== null) {
      setSearchPrefix(null);
      setHistoryIndex(null);
    }
    // Same for the tab-completion cycle; typing anything breaks it.
    if (tabStateRef.current) {
      tabStateRef.current = null;
    }
  };

  // Build the list of completion candidates ordered by source priority:
  //   1. Unique words pulled from typed-command history, most recent first.
  //   2. Room-character names from the latest Room.Chars GMCP push.
  //   3. Capitalized name-like tokens seen anywhere in MUD output in
  //      the last 30 minutes (who-list names, comm-channel speakers,
  //      consider targets, etc.). Populated by Terminal.tsx via
  //      ingestRecentNames().
  // Filter by case-insensitive prefix and deduplicate so the user does
  // not see the same word twice when a noun also appeared in history.
  const buildTabMatches = (prefix: string): string[] => {
    const lower = prefix.toLowerCase();
    const seen = new Set<string>();
    const matches: string[] = [];
    const consider = (word: string) => {
      if (word.length === 0) return;
      if (word.toLowerCase() === lower) return;
      if (!word.toLowerCase().startsWith(lower)) return;
      const key = word.toLowerCase();
      if (seen.has(key)) return;
      seen.add(key);
      matches.push(word);
    };
    for (let i = history.length - 1; i >= 0; i--) {
      for (const token of history[i].split(/\s+/)) {
        consider(token);
      }
    }
    for (const name of roomCharsRef.current) {
      consider(name);
    }
    for (const name of recentNames()) {
      consider(name);
    }
    return matches;
  };

  const handleTabComplete = (step: number) => {
    const el = inputRef.current;
    if (!el) return;
    const caret = el.selectionStart ?? value.length;
    const state = tabStateRef.current;
    if (state) {
      // Cycle within the existing match set.
      if (state.matches.length === 0) return;
      const next = (state.idx + step + state.matches.length) % state.matches.length;
      const match = state.matches[next];
      const before = value.slice(0, state.wordStart);
      const after = value.slice(value.length - state.suffixOffset);
      const nextValue = before + match + after;
      setValue(nextValue);
      tabStateRef.current = { ...state, idx: next };
      // Move the caret to the end of the inserted match on the next
      // tick so React has committed the value update.
      requestAnimationFrame(() => {
        const e2 = inputRef.current;
        if (!e2) return;
        const pos = before.length + match.length;
        e2.setSelectionRange(pos, pos);
      });
      return;
    }
    // Fresh completion. Walk back from caret to find the start of
    // the current word.
    let start = caret;
    while (start > 0 && /\S/.test(value[start - 1])) start -= 1;
    const prefix = value.slice(start, caret);
    if (prefix.length === 0) return;
    const matches = buildTabMatches(prefix);
    if (matches.length === 0) return;
    const idx = step >= 0 ? 0 : matches.length - 1;
    const match = matches[idx];
    const before = value.slice(0, start);
    const after = value.slice(caret);
    const nextValue = before + match + after;
    setValue(nextValue);
    tabStateRef.current = {
      wordStart: start,
      matches,
      idx,
      suffixOffset: after.length,
    };
    requestAnimationFrame(() => {
      const e2 = inputRef.current;
      if (!e2) return;
      const pos = before.length + match.length;
      e2.setSelectionRange(pos, pos);
    });
  };

  // Shared submit path for both Enter and multi-line paste. Echoes the
  // line locally (skipping the echo for quick-keys so the backend's
  // expansion lands at the prompt's cursor), records history, and
  // forwards the line to the backend. Empty lines are forwarded too:
  // pressing Enter on an empty prompt is a valid MUD command on many
  // worlds (re-shows the prompt). The paste handler filters empties
  // before calling so accidental trailing newlines don't flood.
  const submitLine = async (line: string) => {
    if (line.length > 0 && !passwordMode) {
      setHistory((prev) => {
        if (prev[prev.length - 1] === line) return prev;
        return [...prev, line];
      });
    }
    const firstWord = line.split(/\s+/)[0] ?? '';
    const isQuickKey = quickKeysRef.current.some((q) => q.name === firstWord && q.verb.length > 0);
    if (passwordMode) {
      onLocalEcho?.('\r\n');
    } else if (!isQuickKey) {
      onLocalEcho?.(`${line}\r\n`);
    }
    try {
      await sendInput(line);
    } catch (e) {
      onError?.(String(e));
    }
  };

  // Multi-line paste. A single-line `<input>` collapses pasted newlines
  // into spaces by default, so pasting an 8-line sequence ends up as
  // one mangled command. Intercept paste, split on newlines, and send
  // each line as its own command via submitLine. Single-line pastes
  // fall through to the browser default so the cursor and existing
  // input value are preserved. Password mode is exempt so passwords
  // copied with stray whitespace never leak as individual sends.
  //
  // Lines are spread over time using `paste_line_delay_ms` so MUD
  // flood filters do not kick the connection. Esc cancels the queue
  // and leaves any unsent lines unsent. The burst state drives the
  // [paste N/M esc cancels] indicator next to the prompt.
  const pasteCancelRef = useRef<boolean>(false);
  const [pasteBurst, setPasteBurst] = useState<{ sent: number; total: number } | null>(null);
  const handlePaste = async (event: ClipboardEvent<HTMLInputElement>) => {
    if (passwordMode) return;
    const text = event.clipboardData.getData('text');
    if (!text.includes('\n') && !text.includes('\r')) return;
    event.preventDefault();
    const lines = text
      .replace(/\r\n?/g, '\n')
      .split('\n')
      .filter((l) => l.length > 0);
    if (lines.length === 0) return;
    setValue('');
    setSearchPrefix(null);
    setHistoryIndex(null);
    // Cancel any in-flight burst before starting a new one so a fresh
    // paste replaces the queue instead of interleaving with the old
    // remainder.
    pasteCancelRef.current = true;
    await Promise.resolve();
    pasteCancelRef.current = false;
    const delay = pasteDelayRef.current;
    const total = lines.length;
    // Single-line bursts skip the indicator and the delay — they read
    // as a normal Enter to the user.
    if (total === 1) {
      await submitLine(lines[0]);
      return;
    }
    setPasteBurst({ sent: 0, total });
    for (let i = 0; i < total; i++) {
      if (pasteCancelRef.current) break;
      await submitLine(lines[i]);
      setPasteBurst({ sent: i + 1, total });
      if (i < total - 1 && delay > 0) {
        await new Promise<void>((resolve) => {
          const id = window.setTimeout(resolve, delay);
          // Esc-driven cancel cuts the wait short so the indicator
          // clears immediately instead of after the next tick.
          const tick = window.setInterval(() => {
            if (pasteCancelRef.current) {
              window.clearTimeout(id);
              window.clearInterval(tick);
              resolve();
            }
          }, 30);
          window.setTimeout(() => window.clearInterval(tick), delay + 50);
        });
      }
    }
    setPasteBurst(null);
  };

  const handleKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
    // Tab completion. Pressing Tab once builds a candidate list from
    // history words and room characters that prefix-match the word
    // being typed. Pressing Tab again cycles through the matches.
    // Any other key resets the cycle.
    if (event.key === 'Tab') {
      event.preventDefault();
      handleTabComplete(event.shiftKey ? -1 : 1);
      return;
    }
    if (tabStateRef.current) {
      tabStateRef.current = null;
    }

    // Macro lookup runs first so a bound key fires its command
    // regardless of any other handler. allowPlainPrintable matches
    // what the Settings capture path uses, so a binding to a bare
    // character (e.g. "\") fires here too. The lookup is gated by
    // macroMapRef.current.get(canonical), so unbound printable keys
    // still fall through to normal typing.
    const canonical = canonicalKeyFromEvent(event, { allowPlainPrintable: true });
    if (canonical) {
      const command = macroMapRef.current.get(canonical);
      if (command) {
        event.preventDefault();
        try {
          await sendInput(command);
        } catch (e) {
          onError?.(String(e));
        }
        return;
      }
    }

    // Page-scroll the terminal scrollback. macOS sends PageUp/PageDown
    // when the user presses Fn+Up/Fn+Down. Other platforms: PageUp/
    // PageDown directly.
    if (event.key === 'PageUp' || event.key === 'PageDown') {
      event.preventDefault();
      onScrollTerminal?.(event.key === 'PageUp' ? -1 : 1);
      return;
    }

    // Esc cancels an in-flight paste burst first (so the user can stop
    // a 50-line script mid-flight). When no burst is active, it falls
    // through to closing the split-scrollback view; the host ignores
    // the call when nothing is split, so a stray Esc is safe.
    if (event.key === 'Escape') {
      if (pasteBurst) {
        pasteCancelRef.current = true;
        setPasteBurst(null);
        return;
      }
      onExitSplit?.();
      return;
    }

    // Move to start/end of the input line. macOS conventions:
    //   Cmd+Left / Cmd+Right — start / end of line
    //   Fn+Left / Fn+Right   — generate Home / End in browsers
    // Cross-platform Home/End still works.
    //
    // Shift+Home / Shift+End extend the selection from the current
    // caret to the start/end of the input. Without that branch the
    // caret just collapsed and the user lost the selection.
    //
    // Long inputs that overflow horizontally need scrollLeft set
    // explicitly so the caret actually appears at the new position;
    // setSelectionRange alone moves the caret in the document but
    // does not always pan the viewport, leaving the user looking at
    // the old position until the next keystroke.
    if (event.key === 'Home' || (event.metaKey && event.key === 'ArrowLeft')) {
      event.preventDefault();
      const el = inputRef.current;
      if (!el) return;
      if (event.shiftKey) {
        const anchor = el.selectionEnd ?? 0;
        el.setSelectionRange(0, anchor, 'backward');
      } else {
        el.setSelectionRange(0, 0);
      }
      el.scrollLeft = 0;
      return;
    }
    if (event.key === 'End' || (event.metaKey && event.key === 'ArrowRight')) {
      event.preventDefault();
      const el = inputRef.current;
      if (!el) return;
      const end = el.value.length;
      if (event.shiftKey) {
        const anchor = el.selectionStart ?? end;
        el.setSelectionRange(anchor, end, 'forward');
      } else {
        el.setSelectionRange(end, end);
      }
      el.scrollLeft = el.scrollWidth;
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      const line = value;
      const keepLast = keepLastRef.current && line.length > 0 && !passwordMode;
      if (keepLast) {
        // Restore the line and select it after React commits so the
        // user can press Enter to resend. setSelectionRange selects
        // the whole value; the OS will paint the standard text-
        // selection highlight (overridden by .input-row input::
        // selection in styles.css to use the theme accent).
        setValue(line);
        requestAnimationFrame(() => {
          const el = inputRef.current;
          if (el) el.setSelectionRange(0, line.length);
        });
      } else {
        setValue('');
      }
      setSearchPrefix(null);
      setHistoryIndex(null);
      await submitLine(line);
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      const matches = startSearchIfNeeded();
      if (matches.length === 0) return;
      const currentMatchPos =
        historyIndex === null ? matches.length : matches.indexOf(historyIndex);
      const nextPos = Math.max(0, currentMatchPos - 1);
      const next = matches[nextPos];
      if (next === undefined) return;
      setHistoryIndex(next);
      setValue(history[next] ?? '');
      return;
    }

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (historyIndex === null) return;
      const matches = matchingIndices(searchPrefix);
      const currentMatchPos = matches.indexOf(historyIndex);
      const nextPos = currentMatchPos + 1;
      if (nextPos >= matches.length) {
        setHistoryIndex(null);
        setValue(searchPrefix ?? '');
      } else {
        const next = matches[nextPos];
        if (next === undefined) return;
        setHistoryIndex(next);
        setValue(history[next] ?? '');
      }
    }
  };

  return (
    <div className={`input-row${pasteBurst ? ' input-row-pasting' : ''}`}>
      <span className="prompt" aria-hidden="true">
        &gt;
      </span>
      {pasteBurst && (
        <span className="paste-burst" aria-live="polite" aria-label="pasting lines">
          <span className="paste-burst-tag">paste</span>
          <span className="paste-burst-count">
            <span className="paste-burst-sent">{pasteBurst.sent}</span>
            <span className="paste-burst-slash">/</span>
            <span className="paste-burst-total">{pasteBurst.total}</span>
          </span>
          <span className="paste-burst-hint">esc cancels</span>
        </span>
      )}
      <input
        ref={inputRef}
        type={passwordMode ? 'password' : 'text'}
        value={value}
        // Stay enabled even when disconnected so the user can compose
        // commands ahead of a reconnect. The backend echoes
        // [not connected] when Enter fires without a session, which
        // is friendlier than a dead input field.
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete={passwordMode ? 'current-password' : 'off'}
        placeholder={passwordMode ? 'password' : ''}
        aria-label={passwordMode ? 'password input' : 'command input'}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
      />
    </div>
  );
});
