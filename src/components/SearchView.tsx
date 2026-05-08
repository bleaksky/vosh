import { useEffect, useMemo, useState } from 'react';
import {
  exportLogSession,
  listLogSessions,
  searchLogs,
  type LogSearchHit,
  type LogSession,
} from '../lib/session';
import { parseAnsi, styleToCss } from '../lib/ansi';

interface Props {
  onError?: (message: string) => void;
}

function formatDate(ts_ms: number): string {
  const d = new Date(ts_ms);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function formatTime(ts_ms: number): string {
  const d = new Date(ts_ms);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function downloadText(filename: string, content: string) {
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function HitText({ hit }: { hit: LogSearchHit }) {
  if (!hit.raw || hit.raw.length === 0) {
    return <span className="search-hit-text">{hit.text}</span>;
  }
  const bytes = new Uint8Array(hit.raw);
  const chunks = parseAnsi(bytes);
  return (
    <span className="search-hit-text">
      {chunks.map((chunk, i) => (
        <span key={i} style={styleToCss(chunk.style)}>
          {chunk.text}
        </span>
      ))}
    </span>
  );
}

export function SearchView({ onError }: Props) {
  const [pattern, setPattern] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [maxResults, setMaxResults] = useState(500);
  const [sessions, setSessions] = useState<LogSession[]>([]);
  const [sessionFilter, setSessionFilter] = useState<number | null>(null);
  const [hits, setHits] = useState<LogSearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [elapsedMs, setElapsedMs] = useState<number | null>(null);
  const [showTimes, setShowTimes] = useState(false);

  useEffect(() => {
    listLogSessions(50)
      .then(setSessions)
      .catch((e) => onError?.(String(e)));
  }, [onError]);

  const refreshSessions = () => {
    listLogSessions(50)
      .then(setSessions)
      .catch((e) => onError?.(String(e)));
  };

  const runSearch = async () => {
    if (!pattern) {
      setHits([]);
      setElapsedMs(null);
      return;
    }
    setSearching(true);
    const t0 = performance.now();
    try {
      const results = await searchLogs(pattern, {
        caseSensitive,
        maxResults,
        sessionId: sessionFilter,
      });
      setHits(results);
      setElapsedMs(performance.now() - t0);
    } catch (e) {
      onError?.(String(e));
      setHits([]);
      setElapsedMs(null);
    } finally {
      setSearching(false);
    }
  };

  const handleExport = async (sessionId: number, withAnsi: boolean) => {
    try {
      const text = await exportLogSession(sessionId, withAnsi);
      const stamp = sessions.find((s) => s.id === sessionId);
      const ext = withAnsi ? 'ansi.log' : 'log';
      const name = stamp
        ? `${stamp.host}_${stamp.port}_${stamp.id}.${ext}`
        : `session_${sessionId}.${ext}`;
      downloadText(name, text);
    } catch (e) {
      onError?.(String(e));
    }
  };

  // Hits arrive newest-first from the database (so max_results caps the
  // most recent matches). Display each session's hits oldest-first so
  // the transcript reads top-down like a normal log.
  const groupedHits = useMemo(() => {
    const groups: Record<number, LogSearchHit[]> = {};
    for (const hit of hits) {
      if (!groups[hit.session_id]) groups[hit.session_id] = [];
      groups[hit.session_id]!.push(hit);
    }
    for (const sid of Object.keys(groups)) {
      groups[Number(sid)]!.reverse();
    }
    return groups;
  }, [hits]);

  return (
    <section className="search-view" aria-label="log search">
      <header className="pane-header">
        <span>search logs</span>
        <button type="button" className="link-button" onClick={refreshSessions}>
          refresh sessions
        </button>
      </header>

      <div className="search-controls">
        <input
          type="text"
          placeholder="regex pattern"
          value={pattern}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => setPattern(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void runSearch();
            }
          }}
        />
        <div className="search-options-row">
          <label>
            <input
              type="checkbox"
              checked={caseSensitive}
              onChange={(e) => setCaseSensitive(e.target.checked)}
            />
            case
          </label>
          <label>
            <input
              type="checkbox"
              checked={showTimes}
              onChange={(e) => setShowTimes(e.target.checked)}
            />
            time
          </label>
          <label>
            max
            <input
              type="number"
              min={1}
              max={10000}
              value={maxResults}
              onChange={(e) => setMaxResults(Math.max(1, Number(e.target.value) || 1))}
            />
          </label>
        </div>
        <select
          value={sessionFilter ?? ''}
          onChange={(e) =>
            setSessionFilter(e.target.value === '' ? null : Number(e.target.value))
          }
        >
          <option value="">all sessions</option>
          {sessions.map((s) => (
            <option key={s.id} value={s.id}>
              #{s.id} {s.host}:{s.port} ({s.line_count} lines)
            </option>
          ))}
        </select>
        <button type="button" onClick={runSearch} disabled={searching}>
          {searching ? 'searching...' : 'search'}
        </button>
      </div>

      <div className="search-summary">
        {elapsedMs !== null && (
          <span>
            {hits.length} hit{hits.length === 1 ? '' : 's'} in {Math.round(elapsedMs)} ms
          </span>
        )}
      </div>

      <div className="search-results">
        {Object.entries(groupedHits).map(([sid, lines]) => {
          const sessionId = Number(sid);
          const meta = sessions.find((s) => s.id === sessionId);
          return (
            <div key={sid} className="search-session-group">
              <div className="search-session-header">
                <span>
                  session #{sid}
                  {meta ? ` ${meta.host}:${meta.port} ${formatDate(meta.started_at_ms)}` : ''}
                </span>
                <span className="search-session-actions">
                  <button type="button" onClick={() => handleExport(sessionId, false)}>
                    export plain
                  </button>
                  <button type="button" onClick={() => handleExport(sessionId, true)}>
                    export ansi
                  </button>
                </span>
              </div>
              <ul className="search-hit-list">
                {lines.map((hit) => (
                  <li key={hit.line_id} className="search-hit">
                    {showTimes && (
                      <span className="search-hit-time" title={formatDate(hit.ts_ms)}>
                        {formatTime(hit.ts_ms)}
                      </span>
                    )}
                    <HitText hit={hit} />
                  </li>
                ))}
              </ul>
            </div>
          );
        })}
        {!searching && hits.length === 0 && elapsedMs !== null && (
          <div className="search-empty">no matches</div>
        )}
      </div>
    </section>
  );
}
