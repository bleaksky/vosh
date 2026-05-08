import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import { onInputMode, sendInput } from '../lib/session';

export interface InputHandle {
  focus: () => void;
}

interface Props {
  enabled: boolean;
  onError?: (message: string) => void;
  onLocalEcho?: (text: string) => void;
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  { enabled, onError, onLocalEcho }: Props,
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
  };

  const handleKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      const line = value;
      setValue('');
      setSearchPrefix(null);
      setHistoryIndex(null);
      if (line.length > 0 && !passwordMode) {
        setHistory((prev) => {
          if (prev[prev.length - 1] === line) return prev;
          return [...prev, line];
        });
      }
      // Echo synchronously so the typed line appears the same frame the
      // user pressed Enter. The cursor sits at the end of the partial
      // prompt, so the line lands inline (TinTin++ style) and the
      // trailing \r\n moves the cursor to the row where the server
      // response will print. In password mode, only echo a newline so
      // the password itself never lands in the terminal scrollback.
      if (passwordMode) {
        onLocalEcho?.('\r\n');
      } else {
        onLocalEcho?.(`${line}\r\n`);
      }
      try {
        await sendInput(line);
      } catch (e) {
        onError?.(String(e));
      }
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
    <div className="input-row">
      <span className="prompt" aria-hidden="true">
        &gt;
      </span>
      <input
        ref={inputRef}
        type={passwordMode ? 'password' : 'text'}
        value={value}
        disabled={!enabled}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete={passwordMode ? 'current-password' : 'off'}
        placeholder={
          passwordMode ? 'password' : enabled ? 'type a command, or #help' : 'input disabled'
        }
        aria-label={passwordMode ? 'password input' : 'command input'}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
      />
    </div>
  );
});
