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
import { echoMark, markText } from './maskedInput';
import {
  getEchoMarkOptions,
  subscribeEchoMarkOptions,
  useEchoMarkOptions,
} from '../stores/config/echoMarkStore';
import { useLineLook } from '../stores/config/lineLookStore';
import { useLineMark } from '../stores/config/lineMarkStore';
import { useTypeColors } from '../stores/config/typeColorsStore';
import { useKnownWords } from '../stores/session/knownWordsStore';

/** The command line settings. The ones that change what the row draws
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
  // The mark each echo starts with, built from Mark before your commands and
  // the Mark color, and Dim sent commands, which the echo mark store
  // keeps current.
  const echoMarkRef = useRef<string>(echoMark(getEchoMarkOptions()));
  const echoDimRef = useRef<boolean>(getEchoMarkOptions().dim);
  // The mark the row draws before the line you type, while Use the same
  // mark in the command line is on.
  const markOptions = useEchoMarkOptions();
  const lineMarkOn = useLineMark();
  // How the row looks: the caret blink and color, the text color, the
  // background and the size.
  const lineLook = useLineLook();
  // Color commands as you type, and the words Vosh knows that judge it.
  const typeColors = useTypeColors();
  const knownWords = useKnownWords();
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
        const options = getEchoMarkOptions();
        echoMarkRef.current = echoMark(options);
        echoDimRef.current = options.dim;
      }),
    [],
  );

  return {
    spellcheckPrompt,
    cursorStyle,
    lineLook,
    typeColors,
    knownWords,
    lineMark: lineMarkOn ? markText(markOptions) : '',
    keepLastRef,
    pasteDelayRef,
    echoColorRef,
    echoMacrosRef,
    echoMarkRef,
    echoDimRef,
  };
}
