import {
  forwardRef,
  useCallback,
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
import { sendInput, sendMaskedInput, sendRawInput, stopWalk } from '../ipc/session';
import { writingStart } from '../ipc/writing';
import { useWriting, writingOf } from '../stores/session/writingStore';
import { loadWriting, useWritingFile } from '../writing/draftsStore';
import { pasted } from '../writing/text';
import { editorLineOf, heldLine, useFieldCell, washPast } from './editorLine';
import { EditorMarks } from './EditorMarks';
import { canonicalKeyFromEvent } from '../automation/macroKeys';
import {
  draftAfterMaskChange,
  isMasked,
  keepsLastCommand,
  macroEcho,
  planSubmit,
} from './maskedInput';
import { useCaret } from './useCaret';
import { useCommandHistory } from './useCommandHistory';
import { useInputPreferences } from './useInputPreferences';
import type { MacroKeys } from './useMacroKeys';
import { useTabCompletion } from './useTabCompletion';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { isMacPlatform, shortcutKey } from '../lib/shortcuts';
import { getPasswordMode, subscribePasswordMode } from '../stores/session/inputModeStore';
import { getSelected, useSelected } from '../stores/session/sessionsStore';
import { getTargetState } from '../stores/session/targetStore';

export interface InputHandle {
  focus: () => void;
  /** Replace the compose value and focus with the caret at the end.
   *  The palette uses it to hand off parameterized aliases. */
  insert: (text: string) => void;
}

interface Props {
  enabled: boolean;
  /** The selected session's macros, which keys fire here. */
  macroKeys: MacroKeys;
  /** A send to `session` failed. */
  onError?: (message: string, session: number) => void;
  /** Text the command line writes into the terminal of `session`, the
   *  session the line went to. */
  onLocalEcho?: (text: string, session: number) => void;
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

/** The writing card drives the game's editor in `session`, so its other
 *  sends wait. */
function writingHolds(session: number): boolean {
  const job = writingOf(session).job;
  return job !== null && job.action !== 'paste';
}

const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms));

function looksLikeChat(line: string): boolean {
  const trimmed = line.trimStart();
  if (trimmed.length === 0) return false;
  return CHAT_PREFIXES.some((re) => re.test(trimmed));
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  {
    enabled,
    macroKeys,
    onError,
    onLocalEcho,
    onScrollTerminal,
    onExitSplit,
    onSelectAllTerminal,
    fontKey,
  }: Props,
  ref,
) {
  // The one command line types into the selected session. Each line goes
  // to the session that was selected as you sent it, and each session
  // keeps its own history and draft.
  const session = useSelected();
  // The session this command line last showed, which a selection moves
  // before the line shows the next one.
  const sessionRef = useRef(session);
  sessionRef.current = session;
  const [value, setValue] = useState('');
  const [passwordMode, setPasswordMode] = useState(getPasswordMode);
  // The mask from the newest input-mode event. The event sets it at once,
  // while the state above waits for a render. The key, paste, and submit
  // handlers ask maskedNow, which masks when either one says so. The
  // draft a handler holds goes with the last render, so an Enter pressed
  // before the row catches up still treats a password as a password,
  // whether the server just took echo or just handed it back.
  const passwordModeRef = useRef(passwordMode);
  const maskedNow = () => isMasked(passwordMode, passwordModeRef.current);
  const { history, searchPrefix, remember, resetSearch, recallOlder, recallNewer } =
    useCommandHistory(value, setValue, session);
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

  // The masked field follows the selected session: the game's echo in
  // that session, and each selection. The listener reads the newest
  // render's resetSearch. A selection hears the mask of the next session
  // before the line shows it, and that session's own draft comes back
  // with it (useCommandHistory), so only a flip the game makes in the
  // session that shows empties the draft.
  const maskChanged = (password: boolean) => {
    const wasMasked = passwordModeRef.current;
    passwordModeRef.current = password;
    setPasswordMode(password);
    if (wasMasked !== password && sessionRef.current === getSelected()) {
      setValue((draft) => draftAfterMaskChange(wasMasked, password, draft));
      resetSearch();
    }
  };
  const maskChangedRef = useRef(maskChanged);
  maskChangedRef.current = maskChanged;
  useEffect(() => subscribePasswordMode(() => maskChangedRef.current(getPasswordMode())), []);

  const { complete, resetCycle } = useTabCompletion(inputRef, value, setValue, history, session);

  // A line that starts with one of the selected session's quick keys
  // skips the local echo. The backend echoes the expansion (`bash blah`)
  // via session://output, so the shortcut itself never lands in xterm,
  // only the resolved command does.
  const isQuickKey = (word: string) =>
    getTargetState().quick_keys.some((q) => q.name === word && q.verb.length > 0);

  // The game's line editor, open on a text Vosh names, after you kept
  // typing there: each line goes raw, and the line shows the tick and
  // the count. The card's Check spelling covers it, since all you type
  // there is your text.
  const writing = useWriting();
  const editor = editorLineOf(writing);
  const editorRef = useRef(editor);
  editorRef.current = editor;
  const writingFile = useWritingFile();
  const editing = editor !== null;
  useEffect(() => {
    if (editing) void loadWriting();
  }, [editing]);
  const [field, setField] = useState<HTMLTextAreaElement | null>(null);
  // One callback for the life of the row, so a render hands the
  // textarea over once and not on each pass.
  const fieldRef = useCallback((el: HTMLTextAreaElement | null) => {
    inputRef.current = el;
    setField(el);
  }, []);
  const cell = useFieldCell(editing ? field : null);
  const editorText = value.split('\n').pop() ?? '';

  const {
    spellcheckPrompt,
    cursorStyle,
    lineMark,
    keepLastRef,
    pasteDelayRef,
    echoColorRef,
    echoMacrosRef,
    echoMarkRef,
    echoDimRef,
  } = useInputPreferences();

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

  const handleChange = (next: string) => {
    setValue(next);
    // Any direct edit cancels the active prefix search so the next Up uses
    // the current input as the new prefix.
    if (searchPrefix !== null) resetSearch();
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
  const submitLine = async (line: string, to: number) => {
    const masked = maskedNow();
    const firstWord = line.split(/\s+/)[0] ?? '';
    const plan = planSubmit(line, {
      masked,
      quickKey: !masked && isQuickKey(firstWord),
      echoColor: echoColorRef.current,
      echoMark: echoMarkRef.current,
      echoDim: echoDimRef.current,
    });
    if (plan.remember) remember(line, to);
    // #nativesurface is handled here, not in the backend, because the
    // renderer flag lives in localStorage (terminalRenderer.ts reads it
    // at startup). It takes effect on macOS only, where `off` forces
    // xterm and `on` and `default` keep the native surface. Windows and
    // Linux always draw with xterm.
    if (plan.local) {
      const arg = (line.split(/\s+/)[1] ?? '').toLowerCase();
      const notice = (text: string) => onLocalEcho?.(`\x1b[38;5;244m${text}\x1b[0m\r\n`, to);
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
    if (plan.echo !== null) onLocalEcho?.(plan.echo, to);
    try {
      if (plan.masked) await sendMaskedInput(line, to);
      else if (editorRef.current) await sendRawInput(line, to);
      else await sendInput(line, to);
    } catch (e) {
      onError?.(String(e), to);
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
    // In the game's editor a paste wraps and folds as the card's does,
    // keeps its empty lines, and goes on the game's >.
    const open = editorRef.current;
    if (open) {
      const lines = pasted(text, open.width).rows.map((row) => row.text);
      void writingStart({ id: Date.now(), kind: open.kind, action: 'paste', lines }, session).catch(
        (e: unknown) => onError?.(String(e), session),
      );
      return;
    }
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
    // The burst goes to the session you pasted into, even when you
    // select another before it ends.
    const to = session;
    // Single-line bursts skip the indicator and the delay — they read
    // as a normal Enter to the user.
    if (total === 1) {
      await submitLine(lines[0], to);
      return;
    }
    setPasteBurst({ sent: 0, total });
    for (let i = 0; i < total; i++) {
      if (pasteCancelRef.current) break;
      // The writing card holds the session's other sends while it
      // drives the game's editor, a paste in flight among them.
      while (writingHolds(to) && !pasteCancelRef.current) await sleep(100);
      if (pasteCancelRef.current) break;
      await submitLine(lines[i], to);
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
    // Any other key resets the cycle. With no word before the caret
    // Tab moves the focus on, to the panel or back to the terminal.
    if (event.key === 'Tab') {
      if (complete(event.shiftKey ? -1 : 1)) event.preventDefault();
      return;
    }
    resetCycle();

    // Macro lookup runs first so a bound key fires its command
    // regardless of any other handler. allowPlainPrintable matches
    // what the Settings capture path uses, so a binding to a bare
    // character (e.g. "\") fires here too. The lookup is gated by
    // macroKeys.command(canonical), so unbound printable keys
    // still fall through to normal typing.
    const canonical = canonicalKeyFromEvent(event, { allowPlainPrintable: true });
    if (canonical) {
      const command = macroKeys.command(canonical);
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
          quickKey: isQuickKey(firstWord),
          echoColor: echoColorRef.current,
          echoMark: echoMarkRef.current,
          echoDim: echoDimRef.current,
        });
        if (echo !== null) onLocalEcho?.(echo, session);
        try {
          await sendInput(command, session);
        } catch (e) {
          onError?.(String(e), session);
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
        void nativeSurfaceCopy(session).catch(() => {});
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
        void nativeSurfaceSelectAll(session).catch(() => {});
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
        void nativeSurfaceScroll(event.key === 'PageUp' ? 'pageup' : 'pagedown', session).catch(
          () => {},
        );
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
      void stopWalk(session).catch(() => {});
      onExitSplit?.();
      if (nativeSurfaceEnabled()) {
        void nativeSurfaceScroll('bottom', session).catch(() => {});
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
        // input.css to use the theme accent).
        setValue(composed);
        requestAnimationFrame(() => {
          const el = inputRef.current;
          if (el) el.setSelectionRange(0, composed.length);
        });
      } else {
        setValue('');
      }
      resetSearch();
      // Send each composed line as its own command (the Shift+Enter
      // lines, plus the backend still splits `;` within each). A bare
      // Enter on an empty prompt sends one blank line, which advances
      // MUD prompts and paginated output.
      // In the game's editor an empty line is a paragraph break, which
      // goes too.
      const composedLines = editorRef.current
        ? composed.split('\n')
        : composed.split('\n').filter((l) => l.trim().length > 0);
      if (composedLines.length === 0) {
        await submitLine('', session);
      } else {
        for (const cmd of composedLines) {
          await submitLine(cmd, session);
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
      recallOlder();
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
      recallNewer();
    }
  };

  // Logical line count for the gutter. Password prompts are always
  // single-line. The gutter only renders once a second line exists so a
  // normal single command prompt stays clean.
  const lineCount = passwordMode ? 1 : value.split('\n').length;

  return (
    <div className={`input-row${lineCount > 1 ? ' input-row-multiline' : ''}`}>
      {lineMark && (
        <span
          className={[...lineMark].length > 1 ? 'prompt input-mark-wide' : 'prompt'}
          aria-hidden="true"
        >
          {lineMark}
        </span>
      )}
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
          ref={fieldRef}
          rows={1}
          value={value}
          spellCheck={editor ? writingFile.spelling : spellcheckPrompt && looksLikeChat(value)}
          style={editor ? washPast(editorText, editor, cell) : undefined}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
          aria-label="Command line"
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
      {!passwordMode && editor && (
        <EditorMarks field={field} cell={cell} line={editorText} editor={editor} />
      )}
      {writing.held > 0 && (
        <span className="wr-held" aria-live="polite">
          <span className="wr-held-dot dot is-warn" aria-hidden="true" />
          {heldLine(writing.held)}
        </span>
      )}
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
