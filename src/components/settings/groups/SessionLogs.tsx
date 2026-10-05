import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  exportLogSession,
  listLogSessions,
  searchLogPage,
  type LogSearchHit,
  type LogSession,
} from '../../../ipc/logs';
import { resolveThemeTerminalColors } from '../../../ipc/uiConfig';
import { parseHex, toRgba } from '../../../lib/color';
import {
  groupLogDays,
  LOG_PAGE_SIZE,
  logCountText,
  logMatcher,
  logPalette,
  logSessionLabel,
  logSpanCss,
  logTime,
  markMatches,
  parseLogLine,
} from '../../../lib/logView';
import { getCurrentThemeId } from '../../../lib/theme';
import { findTheme } from '../../../lib/themes';
import type { SettingsPageProps } from '../pageTypes';
import { CopyIcon, Field, SearchIcon, Select } from '../ui';

// The log view inside General (the approved SettingsGeneralLogs
// board), at general:logs. One toolbar over the results: the pattern,
// a regular expression over MUD text in the terminal font, the Aa
// match case switch, the count, and the sessions to search. The
// results read oldest first like the terminal and sit scrolled to the
// newest line, under day headings. Each line keeps its own SGR colors
// with your matches marked the way the find bar marks them, and
// earlier matches load as you scroll up. Sessions to 127.0.0.1 and
// localhost stay out, and Copy as text shows once you pick a session.

const ALL = 'all';
// Wait this long after your last keystroke before searching.
const TYPE_DELAY_MS = 250;
// Load earlier lines once you scroll this close to the top.
const LOAD_EARLIER_PX = 600;
const COPIED_MS = 2000;

interface Query {
  pattern: string;
  caseSensitive: boolean;
  sessionId: number | null;
}

type Status =
  | { kind: 'searching' }
  | { kind: 'ready' }
  | { kind: 'bad-pattern' }
  | { kind: 'copied' }
  | { kind: 'failed' };

export function SessionLogs({ config, onError }: SettingsPageProps) {
  const [sessions, setSessions] = useState<LogSession[]>([]);
  const [pattern, setPattern] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [lines, setLines] = useState<LogSearchHit[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  // The query the shown lines answer, which paging continues.
  const [shown, setShown] = useState<Query | null>(null);
  const [status, setStatus] = useState<Status>({ kind: 'searching' });
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const [pinSeq, setPinSeq] = useState(0);
  const seq = useRef(0);
  const resultsRef = useRef<HTMLDivElement | null>(null);
  // Where the view sat before earlier lines went in above it.
  const restoreRef = useRef<{ height: number; top: number } | null>(null);
  const countId = useId();

  useEffect(() => {
    let cancelled = false;
    listLogSessions(0, { hideLocal: true })
      .then((rows) => {
        if (!cancelled) setSessions(rows);
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);

  // Search as you type, and at once when the view opens or the scope
  // or case changes.
  useEffect(() => {
    const mine = ++seq.current;
    setStatus({ kind: 'searching' });
    const query: Query = { pattern, caseSensitive, sessionId };
    const timer = window.setTimeout(
      () => {
        searchLogPage(pattern, {
          caseSensitive,
          maxResults: LOG_PAGE_SIZE,
          sessionId,
          beforeLineId: null,
          hideLocal: true,
          withTotal: true,
        })
          .then((page) => {
            if (mine !== seq.current) return;
            setLines(page.hits);
            setTotal(page.total);
            setShown(query);
            setStatus({ kind: 'ready' });
            setPinSeq((n) => n + 1);
          })
          .catch((e) => {
            if (mine !== seq.current) return;
            const message = String(e);
            setLines([]);
            setTotal(0);
            setShown(query);
            if (message.startsWith('regex')) {
              setStatus({ kind: 'bad-pattern' });
            } else {
              setStatus({ kind: 'failed' });
              onError(message);
            }
          });
      },
      pattern ? TYPE_DELAY_MS : 0,
    );
    return () => window.clearTimeout(timer);
  }, [pattern, caseSensitive, sessionId, onError]);

  // A new result sits scrolled to its newest line.
  useLayoutEffect(() => {
    const el = resultsRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [pinSeq]);

  // Earlier lines went in above. Keep the lines you were reading still.
  useLayoutEffect(() => {
    const el = resultsRef.current;
    const restore = restoreRef.current;
    if (!el || !restore) return;
    restoreRef.current = null;
    el.scrollTop = restore.top + (el.scrollHeight - restore.height);
  }, [lines]);

  const loadEarlier = useCallback(() => {
    const oldest = lines[0];
    if (!shown || !oldest || loadingEarlier || total === null || lines.length >= total) return;
    const mine = seq.current;
    setLoadingEarlier(true);
    searchLogPage(shown.pattern, {
      caseSensitive: shown.caseSensitive,
      maxResults: LOG_PAGE_SIZE,
      sessionId: shown.sessionId,
      beforeLineId: oldest.line_id,
      hideLocal: true,
      withTotal: false,
    })
      .then((page) => {
        if (mine !== seq.current) return;
        if (page.hits.length === 0) {
          // The log ended sooner than the count said. Stop asking.
          setTotal(lines.length);
          return;
        }
        const el = resultsRef.current;
        if (el) restoreRef.current = { height: el.scrollHeight, top: el.scrollTop };
        setLines((prev) => [...page.hits, ...prev]);
      })
      .catch((e) => {
        if (mine === seq.current) onError(String(e));
      })
      .finally(() => {
        if (mine === seq.current) setLoadingEarlier(false);
      });
  }, [lines, shown, loadingEarlier, total, onError]);

  const onScroll = () => {
    const el = resultsRef.current;
    if (el && el.scrollTop < LOAD_EARLIER_PX) loadEarlier();
  };

  const copySession = async () => {
    if (sessionId === null) return;
    try {
      await navigator.clipboard.writeText(await exportLogSession(sessionId, false));
      setStatus({ kind: 'copied' });
    } catch (e) {
      onError(String(e));
    }
  };

  useEffect(() => {
    if (status.kind !== 'copied') return;
    const timer = window.setTimeout(() => setStatus({ kind: 'ready' }), COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [status]);

  // Colors for the lines: the terminal palette the main window uses,
  // and the find bar's mark, ANSI yellow at 28%.
  const themeId = getCurrentThemeId();
  const themeColors = resolveThemeTerminalColors(
    config?.theme ?? themeId,
    config?.theme_terminal_colors ?? null,
  );
  const baseAnsi = config?.terminal_base_ansi ?? null;
  const brightBold = config?.bright_bold ?? false;
  const { palette, mark } = useMemo(() => {
    const xterm = findTheme(themeId).xterm;
    const yellow = parseHex(xterm.yellow);
    return {
      palette: logPalette(xterm, themeColors, baseAnsi),
      mark: yellow ? toRgba(yellow, 0.28) : undefined,
    };
  }, [themeId, themeColors, baseAnsi]);

  const matcher = useMemo(
    () => (shown ? logMatcher(shown.pattern, shown.caseSensitive) : null),
    [shown],
  );
  const days = useMemo(() => {
    const drawn = lines.map((line) => ({
      ...line,
      pieces: markMatches(
        parseLogLine(line.raw && line.raw.length > 0 ? new Uint8Array(line.raw) : line.text),
        matcher,
      ),
    }));
    return groupLogDays(drawn);
  }, [lines, matcher]);

  const scopeOptions = useMemo(
    () => [
      { value: ALL, label: 'All sessions' },
      ...sessions.map((s) => ({ value: String(s.id), label: logSessionLabel(s.started_at_ms) })),
    ],
    [sessions],
  );

  const count = (() => {
    switch (status.kind) {
      case 'searching':
        return lines.length > 0 || shown ? 'Searching…' : '';
      case 'bad-pattern':
        return 'Pattern has an error';
      case 'failed':
        return 'Search failed';
      case 'copied':
        return 'Copied as text';
      case 'ready':
        return logCountText(lines.length, total, shown?.pattern ?? '');
    }
  })();

  return (
    <div className="st-logs">
      <div className="st-logs-bar">
        <Field
          type="search"
          className="st-logs-query"
          icon={<SearchIcon />}
          mono
          autoFocus
          aria-label="Search logs"
          aria-describedby={countId}
          placeholder={sessionId === null ? 'Search all sessions' : 'Search this session'}
          value={pattern}
          onChange={setPattern}
          onKeyDown={(e) => {
            if (e.key === 'Escape' && pattern) {
              e.preventDefault();
              setPattern('');
            }
          }}
        />
        <button
          type="button"
          className="st-glyph-toggle st-glyph-case"
          aria-label="Match case"
          title="Match case"
          aria-pressed={caseSensitive}
          onClick={() => setCaseSensitive((v) => !v)}
        >
          Aa
        </button>
        <span
          id={countId}
          className="st-logs-count"
          role="status"
          data-tone={
            status.kind === 'bad-pattern' || status.kind === 'failed' ? 'danger' : undefined
          }
        >
          {count}
        </span>
        {sessionId !== null && (
          <button
            type="button"
            className="st-icon-button st-logs-copy"
            aria-label="Copy as text"
            title="Copy as text"
            onClick={() => void copySession()}
          >
            <CopyIcon />
          </button>
        )}
        <Select
          className="st-logs-scope"
          aria-label="Sessions to search"
          // A picked session reads like `September 24, 16:07`, which
          // needs more than the board's 160.
          width={sessionId === null ? 160 : 196}
          value={sessionId === null ? ALL : String(sessionId)}
          options={scopeOptions}
          onChange={(v) => setSessionId(v === ALL ? null : Number(v))}
        />
      </div>
      <div
        ref={resultsRef}
        className="st-logs-results"
        tabIndex={0}
        aria-label="Saved lines"
        onScroll={onScroll}
      >
        {status.kind === 'bad-pattern' && (
          <p className="st-logs-empty">
            Vosh reads the pattern as a regular expression and cannot read this one. Check its
            brackets and backslashes.
          </p>
        )}
        {days.length === 0 && status.kind === 'ready' && (
          <p className="st-logs-empty">
            {shown?.pattern
              ? 'No saved line matches that pattern.'
              : sessionId === null
                ? 'Vosh saves every line as you play. It has none saved yet.'
                : 'This session has no saved lines.'}
          </p>
        )}
        {days.map((group) => (
          <section key={group.key} className="st-logs-group" aria-label={group.day}>
            <h2 className="st-logs-day">{group.day}</h2>
            <ul className="st-logs-lines">
              {group.lines.map((line) => (
                <li key={line.line_id} className="st-logs-line">
                  <span className="st-logs-time">{logTime(line.ts_ms)}</span>
                  <span className="st-logs-text">
                    {line.pieces.map((piece, i) => {
                      const css = logSpanCss(piece, palette, brightBold);
                      return piece.match ? (
                        <mark
                          key={i}
                          className="st-logs-match"
                          style={{ ...css, background: mark }}
                        >
                          {piece.text}
                        </mark>
                      ) : (
                        <Fragment key={i}>
                          {Object.keys(css).length > 0 ? (
                            <span style={css}>{piece.text}</span>
                          ) : (
                            piece.text
                          )}
                        </Fragment>
                      );
                    })}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
