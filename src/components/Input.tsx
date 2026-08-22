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
  listMacroGroups,
  onGmcpPackage,
  onInputMode,
  onTarget,
  sendInput,
  subscribeMacroGroupsChanged,
  subscribeMacrosChanged,
  type GroupState,
  type Macro,
  type QuickKey,
} from '../lib/session';
import { canonicalKeyFromEvent } from '../lib/macroKeys';
import { recentNames } from '../lib/recentNames';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { LineChip } from './LineChip';
import { nativeSurfaceEnabled } from './Terminal';

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

// Wrap an echoed input line in a truecolor SGR sequence so the user can
// spot their own commands. Returns the line unchanged when no color is set
// or the hex cannot be parsed. The reset closes before the trailing CRLF.
function colorizeEcho(line: string, color: string | null): string {
  if (!color) return line;
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(color.trim());
  if (!m) return line;
  const r = parseInt(m[1], 16);
  const g = parseInt(m[2], 16);
  const b = parseInt(m[3], 16);
  return `\x1b[38;2;${r};${g};${b}m${line}\x1b[0m`;
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  { enabled, onError, onLocalEcho, onScrollTerminal, onExitSplit }: Props,
  ref,
) {
  const [value, setValue] = useState('');
  const [spellcheckPrompt, setSpellcheckPrompt] = useState(false);
  const [history, setHistory] = useState<string[]>([]);
  const [passwordMode, setPasswordMode] = useState(false);
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
  // Ember block caret. The textarea's native caret is transparent in
  // CSS and replaced with an absolutely-positioned accent block. The
  // position comes from a hidden mirror div cloning the textarea's
  // text up to selectionStart with identical font, padding, and
  // wrapping — no DOM API reports a textarea caret rect directly, so
  // a marker span appended after the cloned text stands in for it.
  // Null hides the block: blurred, password mode (the masked input
  // keeps its native caret), or a non-collapsed selection (the OS
  // paints the selection highlight instead).
  const mirrorRef = useRef<HTMLDivElement | null>(null);
  const caretRef = useRef<HTMLSpanElement | null>(null);
  const [caretPos, setCaretPos] = useState<{ left: number; top: number } | null>(null);

  const measureCaret = () => {
    const el = inputRef.current;
    const mirror = mirrorRef.current;
    if (
      !mirror ||
      !(el instanceof HTMLTextAreaElement) ||
      document.activeElement !== el ||
      el.selectionStart === null ||
      el.selectionStart !== el.selectionEnd
    ) {
      setCaretPos(null);
      return;
    }
    // Rebuild the mirror: text up to the caret (line breaks survive —
    // the mirror is pre-wrap), then the marker. The zero-width space
    // gives the marker the line box's height so the 15px block can
    // center on the line, and cloning the textarea's clientWidth keeps
    // the soft-wrap points identical.
    mirror.style.width = `${el.clientWidth}px`;
    mirror.textContent = el.value.slice(0, el.selectionStart);
    const marker = document.createElement('span');
    marker.textContent = '\u200b';
    mirror.appendChild(marker);
    const left = el.offsetLeft + marker.offsetLeft - el.scrollLeft;
    const top = el.offsetTop + marker.offsetTop - el.scrollTop + (marker.offsetHeight - 15) / 2;
    setCaretPos((prev) => (prev && prev.left === left && prev.top === top ? prev : { left, top }));
  };
  // Latest-closure ref so the document-level listeners below register
  // exactly once instead of resubscribing every render.
  const measureCaretRef = useRef(measureCaret);
  measureCaretRef.current = measureCaret;

  // Re-measure after every commit (edits, the password/textarea swap,
  // programmatic setSelectionRange). The equality guard in setCaretPos
  // keeps the follow-up render from looping.
  useEffect(() => {
    measureCaretRef.current();
  });

  // Caret moves that change no React state (arrow keys, clicks) only
  // surface as document selectionchange; a window resize reflows the
  // row and shifts the wrap points.
  useEffect(() => {
    const remeasure = () => measureCaretRef.current();
    document.addEventListener('selectionchange', remeasure);
    window.addEventListener('resize', remeasure);
    return () => {
      document.removeEventListener('selectionchange', remeasure);
      window.removeEventListener('resize', remeasure);
    };
  }, []);

  // Hold the caret solid while typing: restarting the blink animation
  // on every edit puts it back at the visible half of its keyframe.
  useEffect(() => {
    const el = caretRef.current;
    if (!el) return;
    el.style.animation = 'none';
    void el.offsetWidth;
    el.style.animation = '';
  }, [value]);

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
  }, [value, passwordMode]);

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
  // Color applied to locally-echoed sent input (null = default fg). Same
  // load + subscribe pattern as keepLast.
  const echoColorRef = useRef<string | null>(null);
  // Echo commands sent by keyboard macros like typed input (default
  // on). Under lag the echo shows the keybind registered before the
  // world responds. Same load + subscribe pattern as keepLast.
  const echoMacrosRef = useRef<boolean>(true);
  useEffect(() => {
    let cancelled = false;
    let unlistenKeep: (() => void) | undefined;
    let unlistenPaste: (() => void) | undefined;
    let unlistenSpell: (() => void) | undefined;
    let unlistenEcho: (() => void) | undefined;
    let unlistenEchoMacros: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (cancelled) return;
        keepLastRef.current = cfg.keep_last_command;
        pasteDelayRef.current = cfg.paste_line_delay_ms;
        echoColorRef.current = cfg.input_echo_color;
        echoMacrosRef.current = cfg.echo_macros;
        setSpellcheckPrompt(cfg.spellcheck_prompt);
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
    listen<boolean>('vosh://spellcheck-prompt-changed', (event) => {
      setSpellcheckPrompt(Boolean(event.payload));
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenSpell = fn;
    });
    listen<string | null>('vosh://input-echo-color-changed', (event) => {
      const next = event.payload;
      echoColorRef.current = typeof next === 'string' && next.length > 0 ? next : null;
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenEcho = fn;
    });
    listen<boolean>('vosh://echo-macros-changed', (event) => {
      echoMacrosRef.current = Boolean(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenEchoMacros = fn;
    });
    return () => {
      cancelled = true;
      unlistenKeep?.();
      unlistenPaste?.();
      unlistenSpell?.();
      unlistenEcho?.();
      unlistenEchoMacros?.();
    };
  }, []);

  // Keyboard macro bindings — keyed by canonical key string
  // ("F1", "Ctrl+N", "Numpad7"). Seeded from the backend and
  // refreshed on every macros-changed broadcast. Macros whose
  // group has been bulk-disabled in the Settings UI are stripped
  // here so a disabled "Combat" group means pressing F1 does
  // nothing (key bubbles back up as if no macro existed) instead
  // of firing a stale command.
  const macroMapRef = useRef<Map<string, string>>(new Map());
  const macroListRef = useRef<Macro[]>([]);
  const disabledMacroGroupsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    let cancelled = false;
    const unsubs: Array<() => void> = [];

    const rebuild = () => {
      const m = new Map<string, string>();
      for (const entry of macroListRef.current) {
        if (!entry.key || !entry.command) continue;
        if (entry.group && disabledMacroGroupsRef.current.has(entry.group)) continue;
        m.set(entry.key, entry.command);
      }
      macroMapRef.current = m;
    };

    const applyList = (list: Macro[]) => {
      macroListRef.current = list;
      rebuild();
    };

    const applyGroups = (groups: GroupState[]) => {
      const disabled = new Set<string>();
      for (const g of groups) {
        if (!g.enabled) disabled.add(g.name);
      }
      disabledMacroGroupsRef.current = disabled;
      rebuild();
    };

    const refreshGroups = () => {
      void listMacroGroups()
        .then((groups) => {
          if (!cancelled) applyGroups(groups);
        })
        .catch(() => {});
    };

    listMacros()
      .then((list) => {
        if (!cancelled) applyList(list);
      })
      .catch(() => {});
    refreshGroups();

    subscribeMacrosChanged((list) => {
      if (!cancelled) {
        applyList(list);
        // A new or removed macro can change which groups exist;
        // re-pull the group list so the disabled-set stays accurate.
        refreshGroups();
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });

    subscribeMacroGroupsChanged(() => {
      if (!cancelled) refreshGroups();
    }).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);

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
    // #nativesurface is handled here, not in the backend, because the
    // renderer flag lives in localStorage (Terminal.tsx reads it at
    // startup). `on` forces the native surface on any platform (the
    // Windows/Linux tester path), `off` forces xterm, `default` restores
    // the platform default (native on macOS, xterm elsewhere).
    if (/^#nativesurface\b/i.test(line)) {
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
    const firstWord = line.split(/\s+/)[0] ?? '';
    const isQuickKey = quickKeysRef.current.some((q) => q.name === firstWord && q.verb.length > 0);
    if (passwordMode) {
      onLocalEcho?.('\r\n');
    } else if (!isQuickKey) {
      onLocalEcho?.(`${colorizeEcho(line, echoColorRef.current)}\r\n`);
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
    if (passwordMode) return;
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
        // Echo the macro's command like typed input (same color, same
        // quick-key skip) so under lag the keybind visibly registered
        // before the world responds. The checkbox in Settings turns
        // this off for players whose stacked macros get too noisy.
        if (echoMacrosRef.current && !passwordMode) {
          const firstWord = command.split(/\s+/)[0] ?? '';
          const isQuickKey = quickKeysRef.current.some(
            (q) => q.name === firstWord && q.verb.length > 0,
          );
          if (!isQuickKey) {
            onLocalEcho?.(`${colorizeEcho(command, echoColorRef.current)}\r\n`);
          }
        }
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
        void invoke('native_surface_copy').catch(() => {});
        return;
      }
    }

    // Page-scroll the terminal scrollback. macOS sends PageUp/PageDown
    // when the user presses Fn+Up/Fn+Down. Other platforms: PageUp/
    // PageDown directly.
    if (event.key === 'PageUp' || event.key === 'PageDown') {
      event.preventDefault();
      onScrollTerminal?.(event.key === 'PageUp' ? -1 : 1);
      if (nativeSurfaceEnabled()) {
        void invoke('native_surface_scroll', {
          kind: event.key === 'PageUp' ? 'pageup' : 'pagedown',
        }).catch(() => {});
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
      onExitSplit?.();
      if (nativeSurfaceEnabled()) {
        void invoke('native_surface_scroll', { kind: 'bottom' }).catch(() => {});
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
      if (event.shiftKey && !passwordMode) {
        return;
      }
      event.preventDefault();
      const composed = value;
      const keepLast = keepLastRef.current && composed.length > 0 && !passwordMode;
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
        &raquo;
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
        // continuous spell-checking is enabled at app startup (lib.rs)
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
      <LineChip />
      {!passwordMode && <div className="input-caret-mirror" aria-hidden="true" ref={mirrorRef} />}
      {!passwordMode && caretPos && (
        <span
          ref={caretRef}
          className="input-caret"
          aria-hidden="true"
          style={{ left: caretPos.left, top: caretPos.top }}
        />
      )}
    </div>
  );
});
