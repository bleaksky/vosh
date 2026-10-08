// The command line settings, read at mount and kept current by the
// broadcast each Settings save sends.

import { useEffect, useRef, useState } from 'react';
import {
  getUiConfig,
  normalizeInputCursorStyle,
  subscribeEchoMacrosChanged,
  subscribeInputCursorStyleChanged,
  subscribeInputEchoColorChanged,
  subscribeKeepLastChanged,
  subscribePasteLineDelayChanged,
  subscribeSpellcheckPromptChanged,
  type InputCursorStyle,
} from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { getEchoMarkOptions, subscribeEchoMarkOptions } from '../stores/config/echoMarkStore';

/** The command line settings. The two that change what the row draws
 *  come back as state, and the rest as refs the handlers read when they
 *  run. */
export function useInputPreferences() {
  const [spellcheckPrompt, setSpellcheckPrompt] = useState(false);
  // Caret shape from Settings, general. Only the paint changes — every
  // shape occupies the same anchor box, so the measured position in
  // useCaret.ts stays shape-independent.
  const [cursorStyle, setCursorStyle] = useState<InputCursorStyle>('block');

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
  // Mark your commands, a grey caret before each echo (default on).
  // Same load + subscribe pattern as keepLast.
  const echoCaretRef = useRef<boolean>(getEchoMarkOptions().mark !== 'off');
  useEffect(() => {
    let cancelled = false;
    getUiConfig()
      .then((cfg) => {
        if (cancelled) return;
        keepLastRef.current = cfg.keep_last_command;
        pasteDelayRef.current = cfg.paste_line_delay_ms;
        echoColorRef.current = cfg.input_echo_color;
        echoMacrosRef.current = cfg.echo_macros;
        setSpellcheckPrompt(cfg.spellcheck_prompt);
        setCursorStyle(cfg.input_cursor_style);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);
  useTauriEvent(subscribeKeepLastChanged, (on) => {
    keepLastRef.current = Boolean(on);
  });
  useTauriEvent(subscribePasteLineDelayChanged, (ms) => {
    const n = Number(ms);
    if (Number.isFinite(n) && n >= 0) {
      pasteDelayRef.current = Math.min(10_000, Math.floor(n));
    }
  });
  useTauriEvent(subscribeSpellcheckPromptChanged, (on) => {
    setSpellcheckPrompt(Boolean(on));
  });
  useTauriEvent(subscribeInputCursorStyleChanged, (style) => {
    setCursorStyle(normalizeInputCursorStyle(style));
  });
  useTauriEvent(subscribeInputEchoColorChanged, (next) => {
    echoColorRef.current = typeof next === 'string' && next.length > 0 ? next : null;
  });
  useTauriEvent(subscribeEchoMacrosChanged, (on) => {
    echoMacrosRef.current = Boolean(on);
  });
  useEffect(
    () =>
      subscribeEchoMarkOptions(() => {
        echoCaretRef.current = getEchoMarkOptions().mark !== 'off';
      }),
    [],
  );

  return {
    spellcheckPrompt,
    cursorStyle,
    keepLastRef,
    pasteDelayRef,
    echoColorRef,
    echoMacrosRef,
    echoCaretRef,
  };
}
