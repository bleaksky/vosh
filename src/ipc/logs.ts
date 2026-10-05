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

/** Saved sessions, newest first. A zero limit lists every one.
 *  `hideLocal` leaves out sessions to 127.0.0.1 and localhost. */
export async function listLogSessions(
  limit: number,
  options: { hideLocal?: boolean } = {},
): Promise<LogSession[]> {
  return invoke('logs_list_sessions', { limit, hideLocal: options.hideLocal ?? false });
}

/** One page of a log search, oldest first. */
export interface LogSearchPage {
  hits: LogSearchHit[];
  /** Every line in scope that matches, when the search asked for it. */
  total: number | null;
}

/** The newest `maxResults` lines older than `beforeLineId` that match
 *  `pattern`, a regular expression. An empty pattern matches every
 *  line. `withTotal` also counts every match in that scope, which
 *  reads the whole log. */
export async function searchLogPage(
  pattern: string,
  options: {
    caseSensitive: boolean;
    maxResults: number;
    sessionId: number | null;
    beforeLineId: number | null;
    hideLocal: boolean;
    withTotal: boolean;
  },
): Promise<LogSearchPage> {
  return invoke('logs_search_page', { pattern, ...options });
}

export async function exportLogSession(sessionId: number, withAnsi: boolean): Promise<string> {
  return invoke('logs_export', { sessionId, withAnsi });
}
