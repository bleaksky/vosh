// The typed commands Up and Down recall into the command line, with the
// prefix search over them, and the line you were composing, for each
// session. Rust keeps each session's commands in a file beside its
// scrollback (src-tauri/src/input/history.rs), so a session shows what it
// sent in earlier runs too.

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { historyGet, historyPush } from '../ipc/input';

/** No history yet. */
const NONE: readonly string[] = [];

/** No session list yet. */
const NONE_OPEN: readonly number[] = [];

/** How many commands a session keeps, the newest. Rust cuts each file
 *  to the same number, HISTORY_CAP in input/history.rs. */
export const HISTORY_CAP = 1000;

/** `lines` with `line` at the end, unless it repeats the newest one, and
 *  no more than the cap. The same array when nothing changes. */
export function withLine(lines: readonly string[], line: string): readonly string[] {
  if (lines[lines.length - 1] === line) return lines;
  const next = [...lines, line];
  return next.length > HISTORY_CAP ? next.slice(next.length - HISTORY_CAP) : next;
}

/** What a session sent in earlier runs, then what it sent in this one,
 *  with no command twice in a row and no more than the cap. A line can
 *  reach both when it went out while the earlier ones still loaded. */
export function joinHistory(earlier: readonly string[], now: readonly string[]): readonly string[] {
  let joined: readonly string[] = NONE;
  for (const line of [...earlier, ...now]) joined = withLine(joined, line);
  return joined;
}

/** `lines` without the newest copy of `line`. The same array when it
 *  holds none. */
export function withoutNewest(lines: readonly string[], line: string): readonly string[] {
  const at = lines.lastIndexOf(line);
  return at < 0 ? lines : [...lines.slice(0, at), ...lines.slice(at + 1)];
}

/** The typed history of the command line, which holds `value`, and the
 *  prefix search over it, for the selected session `session`. Recalled
 *  lines go through `setValue`. Input calls remember for each submitted
 *  line, with the session it went to, recallOlder on Up and recallNewer
 *  on Down when the caret has no line left to move to, and resetSearch
 *  when an edit, a submit or a mask change ends the search.
 *
 *  Each session keeps its own history and its own draft. As the
 *  selection moves, the line you were composing stays with the session
 *  you leave, and the command line takes the one the next session holds,
 *  before the page paints. A session reads what it sent in earlier runs
 *  the first time it shows. `open` lists the sessions that are open, and
 *  the history of one that closes goes with it, as its file does. */
export function useCommandHistory(
  value: string,
  setValue: (next: string) => void,
  session: number,
  open: readonly number[] = NONE_OPEN,
) {
  const [histories, setHistories] = useState<ReadonlyMap<number, readonly string[]>>(
    () => new Map(),
  );
  const history = histories.get(session) ?? NONE;
  // When the user starts arrow-key navigation with non-empty input, we
  // remember that prefix so Up and Down cycle only matching history entries.
  // Null means no active prefix search; cycle the full history.
  const [searchPrefix, setSearchPrefix] = useState<string | null>(null);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);

  // The drafts of the sessions behind, and the session whose draft the
  // command line holds.
  const drafts = useRef(new Map<number, string>());
  const shown = useRef(session);
  const valueRef = useRef(value);
  valueRef.current = value;

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

  // The sessions whose earlier commands were asked for, and the newest
  // line handed to Rust for each, so a repeat never lands in the file.
  const loaded = useRef(new Set<number>());
  const pushed = useRef(new Map<number, string>());

  useEffect(() => {
    if (loaded.current.has(session)) return;
    loaded.current.add(session);
    historyGet(session).then(
      (earlier) => {
        if (!Array.isArray(earlier) || earlier.length === 0) return;
        if (!pushed.current.has(session)) pushed.current.set(session, earlier[earlier.length - 1]!);
        setHistories((now) =>
          new Map(now).set(session, joinHistory(earlier, now.get(session) ?? NONE)),
        );
      },
      () => undefined,
    );
  }, [session]);

  useEffect(() => {
    if (open.length === 0) return;
    const live = new Set(open);
    for (const id of [...loaded.current]) {
      if (!live.has(id)) {
        loaded.current.delete(id);
        pushed.current.delete(id);
      }
    }
    setHistories((now) => {
      const gone = [...now.keys()].filter((id) => !live.has(id));
      if (gone.length === 0) return now;
      const next = new Map(now);
      for (const id of gone) next.delete(id);
      return next;
    });
  }, [open]);

  /** Add a line submitted to `to`, unless it repeats its newest one, and
   *  hand it to Rust for the next run. A line Rust turns down, one that
   *  gives the game a password, leaves the history again. */
  const remember = (line: string, to: number) => {
    setHistories((now) => {
      const lines = now.get(to) ?? NONE;
      const next = withLine(lines, line);
      return next === lines ? now : new Map(now).set(to, next);
    });
    if (pushed.current.get(to) === line) return;
    pushed.current.set(to, line);
    historyPush(line, to).then(
      (kept) => {
        if (kept !== false) return;
        pushed.current.delete(to);
        setHistories((now) => {
          const lines = now.get(to) ?? NONE;
          const next = withoutNewest(lines, line);
          return next === lines ? now : new Map(now).set(to, next);
        });
      },
      () => undefined,
    );
  };

  const resetSearch = () => {
    setSearchPrefix(null);
    setHistoryIndex(null);
  };

  useLayoutEffect(() => {
    const left = shown.current;
    if (left === session) return;
    drafts.current.set(left, valueRef.current);
    shown.current = session;
    setValue(drafts.current.get(session) ?? '');
    drafts.current.delete(session);
    setSearchPrefix(null);
    setHistoryIndex(null);
  }, [session, setValue]);

  const recallOlder = () => {
    const matches = startSearchIfNeeded();
    if (matches.length === 0) return;
    const currentMatchPos = historyIndex === null ? matches.length : matches.indexOf(historyIndex);
    const nextPos = Math.max(0, currentMatchPos - 1);
    const next = matches[nextPos];
    if (next === undefined) return;
    setHistoryIndex(next);
    setValue(history[next] ?? '');
  };

  const recallNewer = () => {
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
  };

  return { history, searchPrefix, remember, resetSearch, recallOlder, recallNewer };
}
