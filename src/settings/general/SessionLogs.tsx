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
  saveLog,
  searchLogPage,
  type LogScope,
  type SceneFormat,
  type LogSearchHit,
  type LogSession,
} from '../../ipc/logs';
import { findMarks } from '../../theme/findMarks';
import {
  groupLogDays,
  LOG_PAGE_SIZE,
  LOG_RANGES,
  logCountText,
  logEmptyText,
  logFileName,
  logMatcher,
  logPalette,
  logPlaceholder,
  logRangeScope,
  logSessionLabel,
  type LogRange,
  logSpanCss,
  logTime,
  markMatches,
  parseLogLine,
  savedPalette,
} from './logView';
import { logsWorld } from './scene';
import { useSessionTarget } from '../../stores/session/useConnection';
import { getCurrentThemeId } from '../../theme/theme';
import { findTheme, resolveThemeTerminalColors } from '../../theme/themes';
import type { SettingsPageProps } from '../pageTypes';
import { Button, CheckIcon, CopyIcon, Field, SaveFileIcon, SearchIcon, Select } from '../../ui';
import { MenuItem, MenuSeparator, MenuSurface, type MenuPlacement } from '../../ui/MenuSurface';
import { menuBelow } from '../../ui/menuPlacement';

// The log view inside General (the approved SettingsGeneralLogs
// board), at general:logs. One toolbar over the results: the pattern,
// a regular expression over MUD text in the terminal font, the Aa
// match case switch, the count, and the logs to search. A log is one
// connection, which the store calls a session (Q21). The view reads
// the world the selected session dials, over the last 7 days until you
// pick This session, Last 30 days, All time or one log (D35). The
// results read oldest first like the terminal and sit scrolled to the
// newest line, under day headings. Each line keeps its own SGR colors
// with your matches marked the way the find bar marks them, and
// earlier matches load as you scroll up. Save as file writes what the
// view reads to Downloads as plain text, with the game's colors or as
// one web page, each line starting with its time when Include times is
// checked, and a password line always hidden (D29). Copy as text and
// Save a scene… show once you pick a log. Copy as text hides a password
// line the same way, and Save a scene… opens the scene page on it,
// unless Log sessions is off for the profile, which leaves nothing to
// save.

// A picked log's value in the scope select.
const LOG_PREFIX = 'log:';
// Wait this long after your last keystroke before searching.
const TYPE_DELAY_MS = 250;
// Load earlier lines once you scroll this close to the top.
const LOAD_EARLIER_PX = 600;
const COPIED_MS = 2000;

// The kinds of file Save as file writes, as its menu names them.
const SAVE_FORMATS: readonly { format: SceneFormat; label: string }[] = [
  { format: 'text', label: 'Plain text (.txt)' },
  { format: 'ansi', label: 'With colors (.log)' },
  { format: 'html', label: 'Web page (.html)' },
];

interface Query {
  pattern: string;
  caseSensitive: boolean;
  scope: LogScope;
}

/** The log a scope select value picks, or null for a range. */
function pickedLog(pick: string): number | null {
  return pick.startsWith(LOG_PREFIX) ? Number(pick.slice(LOG_PREFIX.length)) : null;
}

type Status =
  | { kind: 'searching' }
  | { kind: 'ready' }
  | { kind: 'bad-pattern' }
  | { kind: 'copied' }
  | { kind: 'saved'; name: string }
  | { kind: 'failed' };

interface Props extends SettingsPageProps {
  /** Open Save a scene on the log you picked. */
  onSaveScene: (log: number) => void;
}

export function SessionLogs({ config, onError, onSaveScene }: Props) {
  const [sessions, setSessions] = useState<LogSession[]>([]);
  const [pattern, setPattern] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  // A range, or a picked log as `log:<id>`.
  const [pick, setPick] = useState<string>('week');
  const [target] = useSessionTarget();
  const { host, port } = target;
  const logged = logsWorld(config?.log_sessions ?? null, host);
  const sessionId = pickedLog(pick);
  const range = sessionId === null ? (pick as LogRange) : null;
  const [lines, setLines] = useState<LogSearchHit[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  // The query the shown lines answer, which paging continues.
  const [shown, setShown] = useState<Query | null>(null);
  const [status, setStatus] = useState<Status>({ kind: 'searching' });
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const [pinSeq, setPinSeq] = useState(0);
  const [saveMenu, setSaveMenu] = useState<{ at: MenuPlacement; anchor: HTMLElement } | null>(null);
  // Start each saved line with its time. Off each time the view opens.
  const [saveTimes, setSaveTimes] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const seq = useRef(0);
  const resultsRef = useRef<HTMLDivElement | null>(null);
  // Where the view sat before earlier lines went in above it.
  const restoreRef = useRef<{ height: number; top: number } | null>(null);
  const countId = useId();

  useEffect(() => {
    let cancelled = false;
    listLogSessions(0, { host, port })
      .then((rows) => {
        if (!cancelled) setSessions(rows);
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [host, port, onError]);

  // Search as you type, and at once when the view opens or the scope
  // or case changes.
  useEffect(() => {
    const mine = ++seq.current;
    setStatus({ kind: 'searching' });
    const timer = window.setTimeout(
      () => {
        const log = pickedLog(pick);
        const scope: LogScope =
          log === null ? logRangeScope(pick as LogRange, { host, port }) : { log };
        const query: Query = { pattern, caseSensitive, scope };
        searchLogPage(pattern, {
          caseSensitive,
          maxResults: LOG_PAGE_SIZE,
          scope,
          beforeLineId: null,
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
            const message = String(e);
            // A newer search stopped this one.
            if (mine !== seq.current || message.startsWith('stopped')) return;
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
  }, [pattern, caseSensitive, pick, host, port, onError]);

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
      scope: shown.scope,
      beforeLineId: oldest.line_id,
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
        const message = String(e);
        if (mine === seq.current && !message.startsWith('stopped')) onError(message);
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

  // Colors for the lines: the terminal palette the main window uses,
  // and the find bar's mark, ANSI yellow at 28%.
  const themeId = getCurrentThemeId();
  const themeColors = resolveThemeTerminalColors(config?.theme_terminal_colors ?? null);
  const baseAnsi = config?.terminal_base_ansi ?? null;
  const brightBold = config?.bright_bold ?? false;
  const { palette, mark } = useMemo(() => {
    const xterm = findTheme(themeId).xterm;
    return {
      palette: logPalette(xterm, themeColors, baseAnsi),
      mark: findMarks(xterm)?.match,
    };
  }, [themeId, themeColors, baseAnsi]);

  // Save what the view reads now: its scope as the last search took it,
  // so the file holds the lines you see and the ones above them.
  const saveFile = async (format: SceneFormat) => {
    setSaveMenu(null);
    const log = pickedLog(pick);
    const started = sessions.find((s) => s.id === log)?.started_at_ms ?? null;
    const scope: LogScope =
      shown?.scope ?? (log === null ? logRangeScope(pick as LogRange, { host, port }) : { log });
    const filePalette =
      format === 'html' ? savedPalette(palette, findTheme(themeId).xterm, rootRef.current) : null;
    try {
      const name = await saveLog(
        scope,
        { format, times: saveTimes, palette: filePalette },
        logFileName(range, started),
      );
      setStatus({ kind: 'saved', name });
    } catch (e) {
      onError(String(e));
    }
  };

  useEffect(() => {
    if (status.kind !== 'copied' && status.kind !== 'saved') return;
    const timer = window.setTimeout(() => setStatus({ kind: 'ready' }), COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [status]);

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
      ...LOG_RANGES,
      ...sessions.map((s) => ({
        value: `${LOG_PREFIX}${s.id}`,
        label: logSessionLabel(s.started_at_ms),
        group: 'One log',
      })),
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
      case 'saved':
        return `Saved ${status.name} in Downloads`;
      case 'ready': {
        const text = logCountText(lines.length, total, shown?.pattern ?? '');
        return logged ? text : `${text}. Logging is off for this profile`;
      }
    }
  })();

  return (
    <div className="st-logs" ref={rootRef}>
      <div className="st-logs-bar">
        <Field
          type="search"
          className="st-logs-query"
          icon={<SearchIcon />}
          mono
          autoFocus
          aria-label="Search logs"
          aria-describedby={countId}
          placeholder={logPlaceholder(range)}
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
        <button
          type="button"
          className="st-icon-button st-logs-save"
          aria-label="Save as file"
          title="Save as file"
          aria-haspopup="menu"
          aria-expanded={saveMenu !== null}
          onClick={(e) => {
            if (saveMenu) setSaveMenu(null);
            else
              setSaveMenu({
                at: menuBelow(e.currentTarget.getBoundingClientRect()),
                anchor: e.currentTarget,
              });
          }}
        >
          <SaveFileIcon />
        </button>
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
        {sessionId !== null && (
          <Button
            className="st-logs-scene"
            disabled={!logged}
            onClick={() => onSaveScene(sessionId)}
          >
            Save a scene…
          </Button>
        )}
        <Select
          className="st-logs-scope"
          aria-label="Logs to search"
          // A picked log reads like `September 24, 16:07`, which
          // needs more than the board's 160.
          width={sessionId === null ? 160 : 196}
          value={pick}
          options={scopeOptions}
          onChange={setPick}
        />
      </div>
      {saveMenu && (
        <MenuSurface
          label="Save as file"
          at={saveMenu.at}
          anchor={saveMenu.anchor}
          onClose={() => setSaveMenu(null)}
        >
          <MenuItem
            checked={saveTimes}
            onSelect={() => setSaveTimes((on) => !on)}
            trailing={saveTimes ? <CheckIcon className="menu-check" /> : null}
          >
            Include times
          </MenuItem>
          <MenuSeparator />
          {SAVE_FORMATS.map(({ format, label }) => (
            <MenuItem key={format} onSelect={() => void saveFile(format)}>
              {label}
            </MenuItem>
          ))}
        </MenuSurface>
      )}
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
            {shown?.pattern ? 'No saved line matches that pattern.' : logEmptyText(range)}
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
