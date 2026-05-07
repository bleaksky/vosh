import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import { sendInput } from '../lib/session';

export interface InputHandle {
  focus: () => void;
}

interface Props {
  enabled: boolean;
  onError?: (message: string) => void;
}

export const Input = forwardRef<InputHandle, Props>(function Input(
  { enabled, onError }: Props,
  ref,
) {
  const [value, setValue] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (enabled) inputRef.current?.focus();
  }, [enabled]);

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
      if (line.length > 0) {
        setHistory((prev) => {
          if (prev[prev.length - 1] === line) return prev;
          return [...prev, line];
        });
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
      <span className="prompt">&gt;</span>
      <input
        ref={inputRef}
        type="text"
        value={value}
        disabled={!enabled}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete="off"
        placeholder={enabled ? 'type a command, or #help' : 'input disabled'}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
      />
    </div>
  );
});
