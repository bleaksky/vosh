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
  nativeSurfaceCopy,
  nativeSurfaceScroll,
  nativeSurfaceSelectAll,
} from '../ipc/nativeSurface';
import {
  getTarget,
  onInputMode,
  onTarget,
  sendInput,
  sendMaskedInput,
  stopWalk,
  type QuickKey,
} from '../ipc/session';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { canonicalKeyFromEvent } from '../automation/macroKeys';
import {
  draftAfterMaskChange,
  isMasked,
  keepsLastCommand,
  macroEcho,
  planSubmit,
} from './maskedInput';
import { useCaret } from './useCaret';
import { useInputPreferences } from './useInputPreferences';
import { useMacroKeys } from './useMacroKeys';
import { useTabCompletion } from './useTabCompletion';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { isMacPlatform, shortcutKey } from '../lib/shortcuts';

export interface InputHandle {
  focus: () => void;
  /** Replace the compose value and focus with the caret at the end.
   *  The palette uses it to hand off parameterized aliases. */
  insert: (text: string) => void;
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
  /** Select the whole xterm buffer. Cmd+A on an empty command line
   *  calls it, off the native surface, which selects on its own. */
  onSelectAllTerminal?: () => void;
  /** Changes whenever the prompt font family or size changes. The
   *  autosize snap caches a pixel height computed from the current
   *  metrics, so it must re-run when the metrics move or the row
   *  holds a stale height until the next keystroke and the whole
   *  layout shifts when that keystroke lands. */
  fontKey?: string;
}

// Regex set for "is this line chat-like?" — when the toggle in
// Settings is on and one of these matches the current input, the
// webview's native spell-check flips on for the prompt. Otherwise
// MUD verbs like `kill` / `oload` would light up red on every line.
const CHAT_PREFIXES: RegExp[] = [
  /^say\b/i,
  /^'/, // `'hello` = say hello (FL-style say shortcut)
  /^"/, // `"hello` = say hello on some MUDs
  /^tell\s+\S+\s/i,
  /^t\s+\S+\s/i,
  /^reply\b/i,
  /^r\s+/i,
  /^whisper\s+\S+\s/i,
  /^chat\b/i,
  /^gossip\b/i,
  /^;/, // `;hello` = gossip on some servers
  /^ooc\b/i,
  /^clan\b/i,
  /^cb\b/i,
  /^imm(talk)?\b/i,
  /^immchat\b/i,
  /^immtell\b/i,
  /^quote\b/i,
  /^emote\b/i,
  /^pmote\b/i,
];

function looksLikeChat(line: string): boolean {
  const trimmed = line.trimStart();
  if (trimmed.length === 0) return false;
  return CHAT_PREFIXES.some((re) => re.test(trimmed));
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  {
    enabled,
    onError,
    onLocalEcho,
    onScrollTerminal,
    onExitSplit,
    onSelectAllTerminal,
    fontKey,
  }: Props,
  ref,
) {
  const [value, setValue] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  const [passwordMode, setPasswordMode] = useState(false);
  // The mask from the newest input-mode event. The event sets it at once,
  // while the state above waits for a render. The key, paste, and submit
  // handlers ask maskedNow, which masks when either one says so. The
  // draft a handler holds goes with the last render, so an Enter pressed
  // before the row catches up still treats a password as a password,
  // whether the server just took echo or just handed it back.
  const passwordModeRef = useRef(false);
  const maskedNow = () => isMasked(passwordMode, passwordModeRef.current);
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);
  // Line-number gutter next to the multi-line prompt. Kept in its own
  // ref so the textarea's scroll position can be mirrored onto it once a
  // compose grows past the visible cap.
  const gutterRef = useRef<HTMLDivElement | null>(null);
  const { mirrorRef, caretRef, caretPos, measureCaret } = useCaret(inputRef, value);

  useEffect(() => {
    // Refocus on enable and whenever the element swaps between the
    // multi-line textarea and the password <input> so a login prompt
    // never silently drops focus mid-session.
    if (enabled) inputRef.current?.focus();
  }, [enabled, passwordMode]);

  // Autosize the multi-line prompt: grow the textarea to fit the
  // composed lines up to the CSS max-height (then it scrolls), and
  // shrink back as lines are removed. No-op in password mode, where the
  // element is a single-line <input>.
  useEffect(() => {
    const el = inputRef.current;
    if (!(el instanceof HTMLTextAreaElement)) return;
    el.style.height = 'auto';
    // scrollHeight is an integer rounding of fractional line metrics
    // (line-height 1.35), so trusting it raw lets the height flip by
    // one pixel between keystrokes — and the input row, terminal, and
    // panels all reflow with it. Snap to whole text rows instead so
    // the height is a pure function of the line count.
    const cs = getComputedStyle(el);
    const line = parseFloat(cs.lineHeight);
    const pad = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    if (Number.isFinite(line) && line > 0) {
      const rows = Math.max(1, Math.round((el.scrollHeight - pad) / line));
      el.style.height = `${(rows * line + pad).toFixed(2)}px`;
    } else {
      el.style.height = `${el.scrollHeight}px`;
    }
  }, [value, passwordMode, fontKey]);

  useTauriEvent(onInputMode, (payload) => {
    const wasMasked = passwordModeRef.current;
    passwordModeRef.current = payload.password;
    setPasswordMode(payload.password);
    if (wasMasked !== payload.password) {
      setValue((draft) => draftAfterMaskChange(wasMasked, payload.password, draft));
      setSearchPrefix(null);
      setHistoryIndex(null);
    }
  });

  const { complete, resetCycle } = useTabCompletion(inputRef, value, setValue, history);

  // Track configured quick-keys so we can skip the local echo when
  // the user types one. The backend echoes the expansion (`bash
  // blah`) via session://output, so the shortcut itself never lands
  // in xterm — only the resolved command does.
  const quickKeysRef = useRef<QuickKey[]>([]);
  useEffect(() => {
    let cancelled = false;
    getTarget()
      .then((snap) => {
        if (!cancelled) quickKeysRef.current = snap.quick_keys;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);
  useTauriEvent(onTarget, (payload) => {
    quickKeysRef.current = payload.quick_keys;
  });

  const {
    spellcheckPrompt,
    cursorStyle,
    keepLastRef,
    pasteDelayRef,
    echoColorRef,
    echoMacrosRef,
    echoCaretRef,
  } = useInputPreferences();

  const macroMapRef = useMacroKeys();

  useImperativeHandle(
    ref,
    () => ({
      focus: () => inputRef.current?.focus(),
      insert: (text: string) => {
        setValue(text);
        const el = inputRef.current;
        el?.focus();
        window.requestAnimationFrame(() => {
          el?.setSelectionRange(text.length, text.length);
        });
      },
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
    resetCycle();
  };

  // Shared submit path for both Enter and multi-line paste. Echoes the
  // line locally (skipping the echo for quick-keys so the backend's
  // expansion lands at the prompt's cursor), records history, and
  // forwards the line to the backend. Empty lines are forwarded too:
  // pressing Enter on an empty prompt is a valid MUD command on many
  // worlds (re-shows the prompt). The paste handler filters empties
  // before calling so accidental trailing newlines don't flood.
  //
  // A line from the masked password field leaves no trace. planSubmit
  // gives it a bare line break for an echo, keeps it out of history, and
  // routes it through the masked send, which skips the input pipeline.
  const submitLine = async (line: string) => {
    const masked = maskedNow();
    const firstWord = line.split(/\s+/)[0] ?? '';
    const plan = planSubmit(line, {
      masked,
      quickKey:
        !masked && quickKeysRef.current.some((q) => q.name === firstWord && q.verb.length > 0),
      echoColor: echoColorRef.current,
      echoCaret: echoCaretRef.current,
    });
    if (plan.remember) {
      setHistory((prev) => {
        if (prev[prev.length - 1] === line) return prev;
        return [...prev, line];
      });
    }
    // #nativesurface is handled here, not in the backend, because the
    // renderer flag lives in localStorage (Terminal.tsx reads it at
    // startup). `on` forces the native surface on any platform (the
    // Windows/Linux tester path), `off` forces xterm, `default` restores
    // the platform default (native on macOS, xterm elsewhere).
    if (plan.local) {
      const arg = (line.split(/\s+/)[1] ?? '').toLowerCase();
      const notice = (text: string) => onLocalEcho?.(`\x1b[38;5;244m${text}\x1b[0m\r\n`);
      if (arg === 'on' || arg === 'off') {
        localStorage.setItem('vosh.nativesurface', arg === 'on' ? '1' : '0');
        notice(`native renderer forced ${arg}. restart Vosh to apply.`);
      } else if (arg === 'default') {
        localStorage.removeItem('vosh.nativesurface');
        notice('native renderer follows the platform default. restart Vosh to apply.');
      } else {
        notice('usage #nativesurface on | off | default (takes effect on restart)');
      }
      return;
    }
    if (plan.echo !== null) onLocalEcho?.(plan.echo);
    try {
      await (plan.masked ? sendMaskedInput(line) : sendInput(line));
    } catch (e) {
      onError?.(String(e));
    }
  };

  // Multi-line paste. A single-line `<input>` collapses pasted newlines
  // into spaces by default, so pasting an 8-line sequence ends up as
  // one mangled command. Intercept paste, split on newlines, and send
  // each line as its own command via submitLine, leaving whatever the
  // user already composed in the prompt intact. Single-line pastes fall
  // through to the browser default so they insert at the cursor.
  // Password mode is exempt so passwords copied with stray whitespace
  // never leak as individual sends.
  //
  // Lines are spread over time using `paste_line_delay_ms` so MUD
  // flood filters do not kick the connection. Esc cancels the queue
  // and leaves any unsent lines unsent. The burst state drives the
  // [paste N/M esc cancels] indicator next to the prompt.
  const pasteCancelRef = useRef<boolean>(false);
  const [pasteBurst, setPasteBurst] = useState<{ sent: number; total: number } | null>(null);
  const handlePaste = async (event: ClipboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    if (maskedNow()) return;
    const text = event.clipboardData.getData('text');
    if (!text.includes('\n') && !text.includes('\r')) return;
    event.preventDefault();
    const lines = text
      .replace(/\r\n?/g, '\n')
      .split('\n')
      .filter((l) => l.length > 0);
    if (lines.length === 0) return;
    // Leave the command line untouched. A multi-line paste force-sends
    // its lines, but whatever the user had already composed (selected
    // or not) stays in the prompt instead of being wiped — only the
    // paste itself is preventDefault'd, so the existing input value and
    // selection survive the burst.
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

  const handleKeyDown = async (event: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    // Tab completion. Pressing Tab once builds a candidate list from
    // history words and room characters that prefix-match the word
    // being typed. Pressing Tab again cycles through the matches.
    // Any other key resets the cycle.
    if (event.key === 'Tab') {
      event.preventDefault();
      complete(event.shiftKey ? -1 : 1);
      return;
    }
    resetCycle();

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
        // Echo the macro's command like typed input (same color, same
        // quick-key skip) so under lag the keybind visibly registered
        // before the world responds. The checkbox in Settings turns
        // this off for players whose stacked macros get too noisy.
        const firstWord = command.split(/\s+/)[0] ?? '';
        const echo = macroEcho(command, {
          enabled: echoMacrosRef.current,
          masked: maskedNow(),
          quickKey: quickKeysRef.current.some((q) => q.name === firstWord && q.verb.length > 0),
          echoColor: echoColorRef.current,
          echoCaret: echoCaretRef.current,
        });
        if (echo !== null) onLocalEcho?.(echo);
        try {
          await sendInput(command);
        } catch (e) {
          onError?.(String(e));
        }
        return;
      }
    }

    // Cmd/Ctrl+C copies the native terminal selection, but only when the
    // input box has nothing selected (so its own copy still works).
    if (
      (event.metaKey || event.ctrlKey) &&
      (event.key === 'c' || event.key === 'C') &&
      nativeSurfaceEnabled()
    ) {
      const el = event.currentTarget;
      const inputHasSelection = el.selectionStart != null && el.selectionStart !== el.selectionEnd;
      if (!inputHasSelection) {
        event.preventDefault();
        void nativeSurfaceCopy().catch(() => {});
        return;
      }
    }

    // Cmd+A (Ctrl+A off macOS) on an empty command line selects the
    // whole terminal, the keyboard twin of the terminal menu's Select
    // all, so Cmd+A then Cmd+C copies the scrollback. With text in the
    // line it selects that text as usual, and Ctrl+A on macOS stays the
    // move to line start.
    const primary = isMacPlatform()
      ? event.metaKey && !event.ctrlKey
      : event.ctrlKey && !event.metaKey;
    if (
      primary &&
      !event.altKey &&
      !event.shiftKey &&
      shortcutKey(event) === 'a' &&
      event.currentTarget.value.length === 0
    ) {
      event.preventDefault();
      if (nativeSurfaceEnabled()) {
        void nativeSurfaceSelectAll().catch(() => {});
      } else {
        onSelectAllTerminal?.();
      }
      return;
    }

    // Page-scroll the terminal scrollback. macOS sends PageUp/PageDown
    // when the user presses Fn+Up/Fn+Down. Other platforms: PageUp/
    // PageDown directly.
    if (event.key === 'PageUp' || event.key === 'PageDown') {
      event.preventDefault();
      onScrollTerminal?.(event.key === 'PageUp' ? -1 : 1);
      if (nativeSurfaceEnabled()) {
        void nativeSurfaceScroll(event.key === 'PageUp' ? 'pageup' : 'pagedown').catch(() => {});
      }
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
      void stopWalk().catch(() => {});
      onExitSplit?.();
      if (nativeSurfaceEnabled()) {
        void nativeSurfaceScroll('bottom').catch(() => {});
      }
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
      // Shift+Enter composes another line in the multi-line prompt
      // instead of submitting. Password mode is single-line, so a
      // Shift+Enter there still submits like a plain Enter.
      if (event.shiftKey && !maskedNow()) {
        return;
      }
      event.preventDefault();
      const composed = value;
      const keepLast = keepsLastCommand(keepLastRef.current, composed, maskedNow());
      if (keepLast) {
        // Restore the composed value and select it after React commits
        // so the user can press Enter to resend. setSelectionRange
        // selects the whole value; the OS paints the standard text-
        // selection highlight (overridden by the ::selection rule in
        // styles.css to use the theme accent).
        setValue(composed);
        requestAnimationFrame(() => {
          const el = inputRef.current;
          if (el) el.setSelectionRange(0, composed.length);
        });
      } else {
        setValue('');
      }
      setSearchPrefix(null);
      setHistoryIndex(null);
      // Send each composed line as its own command (the Shift+Enter
      // lines, plus the backend still splits `;` within each). A bare
      // Enter on an empty prompt sends one blank line, which advances
      // MUD prompts and paginated output.
      const composedLines = composed.split('\n').filter((l) => l.trim().length > 0);
      if (composedLines.length === 0) {
        await submitLine('');
      } else {
        for (const cmd of composedLines) {
          await submitLine(cmd);
        }
      }
      return;
    }

    if (event.key === 'ArrowUp') {
      // In a multi-line compose, move the caret up a line unless it is
      // already on the first line; only then recall command history.
      const elUp = inputRef.current;
      if (elUp && value.slice(0, elUp.selectionStart ?? 0).includes('\n')) {
        return;
      }
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
      // Mirror ArrowUp: move the caret down within a multi-line compose
      // unless it is already on the last line.
      const elDown = inputRef.current;
      if (elDown && value.slice(elDown.selectionStart ?? value.length).includes('\n')) {
        return;
      }
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

  // Logical line count for the gutter. Password prompts are always
  // single-line. The gutter only renders once a second line exists so a
  // normal single command prompt stays clean.
  const lineCount = passwordMode ? 1 : value.split('\n').length;

  return (
    <div
      className={`input-row${pasteBurst ? ' input-row-pasting' : ''}${
        lineCount > 1 ? ' input-row-multiline' : ''
      }`}
    >
      <span className="prompt" aria-hidden="true">
        &#8250;
      </span>
      {lineCount > 1 && (
        <div className="input-gutter" aria-hidden="true" ref={gutterRef}>
          {Array.from({ length: lineCount }, (_, i) => (
            <span key={i}>{i + 1}</span>
          ))}
        </div>
      )}
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
      {passwordMode ? (
        // Password prompts (server WILL ECHO) stay a single-line masked
        // <input>. Multi-line composing never applies to a password, and
        // type="password" gives real masking that a textarea cannot.
        <input
          ref={(el) => {
            inputRef.current = el;
          }}
          type="password"
          value={value}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="current-password"
          placeholder="password"
          aria-label="password input"
          onChange={(e) => handleChange(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
        />
      ) : (
        // Multi-line command prompt. Shift+Enter adds a line and the
        // textarea autosizes; plain Enter submits each line as its own
        // command. Stays enabled even when disconnected so the user can
        // compose ahead of a reconnect — the backend echoes
        // [not connected] when Enter fires without a session.
        //
        // Spell-check fires when (a) the user opted in via Settings AND
        // (b) the current line matches a chat verb. WKWebView's
        // continuous spell-checking is enabled at app startup (app/launch.rs)
        // so the attribute triggers the squiggle pass; plain MUD
        // commands skip it so the prompt stays clean.
        <textarea
          ref={(el) => {
            inputRef.current = el;
          }}
          rows={1}
          value={value}
          spellCheck={spellcheckPrompt && looksLikeChat(value)}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
          aria-label="command input"
          onChange={(e) => handleChange(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          // The block caret exists only while the textarea is focused;
          // blur measures once more and hides it.
          onFocus={measureCaret}
          onBlur={measureCaret}
          // Keep the line-number gutter aligned when a tall compose
          // scrolls inside the capped textarea, and re-measure so the
          // block caret tracks the scrolled text.
          onScroll={(e) => {
            if (gutterRef.current) gutterRef.current.scrollTop = e.currentTarget.scrollTop;
            measureCaret();
          }}
        />
      )}
      {!passwordMode && <div className="input-caret-mirror" aria-hidden="true" ref={mirrorRef} />}
      {!passwordMode && caretPos && (
        <span
          ref={caretRef}
          className={`input-caret caret-shape--${cursorStyle}`}
          aria-hidden="true"
          style={{ left: caretPos.left, top: caretPos.top }}
        />
      )}
    </div>
  );
});
