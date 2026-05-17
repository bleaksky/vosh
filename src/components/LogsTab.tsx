import { useEffect, useState } from 'react';
import {
  exportLogSession,
  listLogSessions,
  searchLogs,
  type LogSearchHit,
  type LogSession,
} from '../lib/session';

interface Props {
  onError: (e: string | null) => void;
}

function formatTime(ms: number): string {
  const d = new Date(ms);
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, '0');
  const da = String(d.getDate()).padStart(2, '0');
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${y}-${mo}-${da} ${hh}:${mm}`;
}

function formatHit(ms: number): string {
  const d = new Date(ms);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

// Full-text search across every persisted session log. Backend pipes
// these into a SQLite store on every connection; this tab is the
// only frontend reader. Sessions list on the left, search input +
// results on the right.
export function LogsTab({ onError }: Props) {
  const [sessions, setSessions] = useState<LogSession[]>([]);
  const [sessionFilter, setSessionFilter] = useState<number | null>(null);
  const [pattern, setPattern] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [hits, setHits] = useState<LogSearchHit[]>([]);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    let cancelled = false;
    listLogSessions(50)
      .then((rows) => {
        if (!cancelled) setSessions(rows);
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);

  const runSearch = async () => {
    if (!pattern.trim()) {
      setHits([]);
      return;
    }
    setSearching(true);
    try {
      const opts: {
        caseSensitive?: boolean;
        maxResults?: number;
        sessionId?: number | null;
      } = { caseSensitive, maxResults: 500 };
      if (sessionFilter !== null) opts.sessionId = sessionFilter;
      const rows = await searchLogs(pattern, opts);
      setHits(rows);
    } catch (e) {
      onError(String(e));
    } finally {
      setSearching(false);
    }
  };

  const exportSession = async (id: number, withAnsi: boolean) => {
    try {
      const text = await exportLogSession(id, withAnsi);
      // Stream to a blob + open in a new tab via data URL would be
      // ideal; for now just copy to clipboard so the user can paste
      // it wherever.
      await navigator.clipboard.writeText(text);
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="logs-tab">
      <div className="logs-sessions">
        <div className="logs-section-label">sessions ({sessions.length})</div>
        <div className="logs-session-list">
          <button
            type="button"
            className={`logs-session${sessionFilter === null ? ' is-active' : ''}`}
            onClick={() => setSessionFilter(null)}
          >
            <span className="logs-session-host">all sessions</span>
          </button>
          {sessions.map((s) => (
            <button
              key={s.id}
              type="button"
              className={`logs-session${sessionFilter === s.id ? ' is-active' : ''}`}
              onClick={() => setSessionFilter(s.id)}
            >
              <span className="logs-session-host">
                {s.host}:{s.port}
              </span>
              <span className="logs-session-meta">
                {formatTime(s.started_at_ms)} · {s.line_count} lines
              </span>
              <span className="logs-session-actions">
                <span
                  className="logs-session-export"
                  role="button"
                  tabIndex={0}
                  onClick={(e) => {
                    e.stopPropagation();
                    void exportSession(s.id, false);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.stopPropagation();
                      void exportSession(s.id, false);
                    }
                  }}
                  title="copy plain text to clipboard"
                >
                  copy
                </span>
              </span>
            </button>
          ))}
        </div>
      </div>
      <div className="logs-search">
        <form
          className="logs-search-form"
          onSubmit={(e) => {
            e.preventDefault();
            void runSearch();
          }}
        >
          <input
            type="text"
            spellCheck={false}
            placeholder={`search ${sessionFilter === null ? 'all sessions' : 'this session'}`}
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
          />
          <label className="logs-case">
            <input
              type="checkbox"
              checked={caseSensitive}
              onChange={(e) => setCaseSensitive(e.target.checked)}
            />
            case
          </label>
          <button type="submit" className="settings-btn">
            [search]
          </button>
          <span className="logs-status">
            {searching ? 'searching…' : `${hits.length} hit${hits.length === 1 ? '' : 's'}`}
          </span>
        </form>
        <div className="logs-results">
          {hits.length === 0 && !searching && (
            <div className="settings-font-empty">
              {pattern ? 'no matches' : 'enter a pattern and hit search'}
            </div>
          )}
          {hits.map((h) => (
            <div key={`${h.session_id}-${h.line_id}`} className="logs-hit">
              <span className="logs-hit-ts">{formatHit(h.ts_ms)}</span>
              <span className="logs-hit-session">
                {h.host}:{h.port}
              </span>
              <span className="logs-hit-text">{h.text}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
