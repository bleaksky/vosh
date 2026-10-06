import { useLayoutEffect, useRef, useState } from 'react';
import { luaOutputClear, luaRun, type LuaKind, type LuaLine } from '../../ipc/scripts';
import { errorText } from '../../lib/text';
import { Button, Field, Section, cx } from '../../ui';
import { clockTime } from './clockTime';

// The Console section of Scripts (boards 2 and 4). Every [lua] line the
// selected session printed, whoever's Lua it is about, so loose scripts
// and #lua lines show beside your plugins, each with its local time.
// The field under them runs Lua there as a #lua line does.

/** The well's note before any line. */
export const NO_LUA_LINES =
  'Every print and error from your plugins shows here and in the terminal.';

/** The class each kind of line takes. A print keeps the text color. */
const KIND_CLASS: Readonly<Record<LuaKind, string | undefined>> = {
  print: undefined,
  error: 'is-error',
  note: 'is-note',
  input: 'is-input',
};

interface Props {
  lines: LuaLine[];
  /** Let go of the page's lines once Clear empties the session's. */
  onCleared: () => void;
  onError: (message: string | null) => void;
}

export function LuaConsole({ lines, onCleared, onError }: Props) {
  const [code, setCode] = useState('');
  const listRef = useRef<HTMLOListElement | null>(null);
  // Whether the list shows its newest line, so a new one scrolls into
  // view unless you scrolled up to read.
  const atEnd = useRef(true);

  useLayoutEffect(() => {
    const list = listRef.current;
    // An empty Console starts at its newest line again.
    if (!list) atEnd.current = true;
    else if (atEnd.current) list.scrollTop = list.scrollHeight;
  }, [lines]);

  const clear = () => {
    luaOutputClear()
      .then(() => {
        onCleared();
        onError(null);
      })
      .catch((e: unknown) => onError(errorText(e)));
  };

  // The line you ran comes back in the list, so the field empties once
  // it runs, unless you typed on meanwhile.
  const run = () => {
    const sent = code;
    if (sent.trim() === '') return;
    luaRun(sent)
      .then(() => {
        setCode((now) => (now === sent ? '' : now));
        onError(null);
      })
      .catch((e: unknown) => onError(errorText(e)));
  };

  const empty = lines.length === 0;
  return (
    <Section
      title="Console"
      id="console"
      card={false}
      actions={
        <Button className="st-auto-quiet" disabled={empty} onClick={clear}>
          Clear
        </Button>
      }
    >
      <div className={cx('st-lua-out', empty && 'is-empty')}>
        {empty ? (
          <p className="st-lua-empty">{NO_LUA_LINES}</p>
        ) : (
          <ol
            ref={listRef}
            className="st-lua-lines"
            onScroll={(e) => {
              const list = e.currentTarget;
              atEnd.current = list.scrollHeight - list.scrollTop - list.clientHeight < 1;
            }}
          >
            {lines.map((line, i) => (
              <li key={i} className={cx('st-lua-line', KIND_CLASS[line.kind])}>
                <span className="st-lua-time">{clockTime(line.ts_ms)}</span>
                <span className="st-lua-text">
                  {line.kind === 'input' ? (
                    `› ${line.text}`
                  ) : (
                    <>
                      <span className="st-lua-tag">[lua]</span> {line.text}
                    </>
                  )}
                </span>
              </li>
            ))}
          </ol>
        )}
        <div className="st-lua-prompt">
          <span className="st-lua-glyph" aria-hidden="true">
            ›
          </span>
          <Field
            mono
            width="100%"
            value={code}
            onChange={setCode}
            placeholder="Run Lua"
            aria-label="Run Lua"
            onKeyDown={(e) => {
              if (e.key !== 'Enter') return;
              e.preventDefault();
              run();
            }}
          />
        </div>
      </div>
    </Section>
  );
}
