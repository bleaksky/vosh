// The session logs, searching them, and exporting one.

import { invoke } from '@tauri-apps/api/core';

export interface LogSession {
  id: number;
  host: string;
  port: number;
  started_at_ms: number;
  ended_at_ms: number | null;
  line_count: number;
}

export interface LogSearchHit {
  session_id: number;
  host: string;
  port: number;
  line_id: number;
  ts_ms: number;
  text: string;
  raw: number[] | null;
}

/** Which logs a list or a search reads. Every part left out reads
 *  every log. */
export interface LogScope {
  /** One log, by its id. */
  log?: number | null;
  /** The logs the selected session opened since Vosh started. */
  thisSession?: boolean;
  /** Only the logs of connections to this host and port. */
  host?: string | null;
  port?: number | null;
  /** Only the lines at or after this time, in Unix ms. */
  sinceMs?: number | null;
  /** Leave out logs of connections to 127.0.0.1 and localhost. */
  hideLocal?: boolean;
}

/** Saved logs in `scope`, newest first. A zero limit lists every one. */
export async function listLogSessions(limit: number, scope: LogScope = {}): Promise<LogSession[]> {
  return invoke('logs_list_sessions', { limit, scope });
}

/** One page of a log search, oldest first. */
export interface LogSearchPage {
  hits: LogSearchHit[];
  /** Every line in scope that matches, when the search asked for it. */
  total: number | null;
}

/** The newest `maxResults` lines in `scope` older than `beforeLineId`
 *  that match `pattern`, a regular expression. An empty pattern
 *  matches every line. `withTotal` also counts every match in that
 *  scope, which reads all of it. Each search stops the one before,
 *  which then fails with an error that starts `stopped`. */
export async function searchLogPage(
  pattern: string,
  options: {
    caseSensitive: boolean;
    maxResults: number;
    scope: LogScope;
    beforeLineId: number | null;
    withTotal: boolean;
  },
): Promise<LogSearchPage> {
  return invoke('logs_search_page', { pattern, ...options });
}

export async function exportLogSession(sessionId: number, withAnsi: boolean): Promise<string> {
  return invoke('logs_export', { sessionId, withAnsi });
}

/** Save the lines in `scope` to the Downloads folder as `<name>.txt`,
 *  or with `withAnsi` as `<name>.log` with the game's colors. Resolves
 *  to the name of the file it wrote, which gains ` (2)` and on when the
 *  name is taken. */
export async function saveLog(scope: LogScope, withAnsi: boolean, name: string): Promise<string> {
  return invoke('logs_save', { scope, withAnsi, name });
}
