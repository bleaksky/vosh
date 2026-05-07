import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { sendLine } from '../lib/session';

interface Props {
  enabled: boolean;
  onError?: (message: string) => void;
}

export function Input({ enabled, onError }: Props) {
  const [value, setValue] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (enabled) inputRef.current?.focus();
  }, [enabled]);

  const handleKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      const line = value;
      setValue('');
      if (line.length > 0) {
        setHistory((prev) => {
          if (prev[prev.length - 1] === line) return prev;
          return [...prev, line];
        });
      }
      setHistoryIndex(null);
      try {
        await sendLine(line);
      } catch (e) {
        onError?.(String(e));
      }
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (history.length === 0) return;
      const next = historyIndex === null ? history.length - 1 : Math.max(0, historyIndex - 1);
      setHistoryIndex(next);
      setValue(history[next] ?? '');
      return;
    }

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (history.length === 0 || historyIndex === null) return;
      const next = historyIndex + 1;
      if (next >= history.length) {
        setHistoryIndex(null);
        setValue('');
      } else {
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
        placeholder={enabled ? 'type a command' : 'connect first'}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
      />
    </div>
  );
}
