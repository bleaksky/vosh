// The session logs, searching them, exporting one, and saving a stretch
// of one as a scene.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SCENE_SAVED } from './events';

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

/** How many days Vosh keeps a log, or null to keep it forever. */
export async function logsKeepGet(): Promise<number | null> {
  return invoke('logs_keep_get');
}

/** Keep logs for 365, 90 or 30 days, or forever with null. Vosh then
 *  deletes the logs past the span. */
export async function logsKeepSet(days: number | null): Promise<void> {
  return invoke('logs_keep_set', { days });
}

/** One log as text for Copy as text, or with `withAnsi` with the
 *  game's colors. A password line comes back hidden, as `> (hidden)`. */
export async function exportLogSession(sessionId: number, withAnsi: boolean): Promise<string> {
  return invoke('logs_export', { sessionId, withAnsi });
}

/** How Save as file writes the lines: the kind of file, whether each
 *  line starts with its time, and for an HTML page the theme showing. */
export interface SaveLogOptions {
  format: SceneFormat;
  times: boolean;
  palette: ScenePalette | null;
}

/** Save the lines in `scope` to the Downloads folder as `<name>.txt`,
 *  `<name>.log` with the game's colors or `<name>.html` as one page.
 *  Resolves to the name of the file it wrote, which gains ` (2)` and on
 *  when the name is taken. A password line is always saved hidden. */
export async function saveLog(
  scope: LogScope,
  options: SaveLogOptions,
  name: string,
): Promise<string> {
  return invoke('logs_save', { scope, options, name });
}

// ── Save a scene ────────────────────────────────────────────────────

/** The stretch of play a scene takes: one log from one time to another,
 *  both ends kept, and from and to a line you clicked. */
export interface SceneRange {
  log: number;
  fromMs: number;
  toMs: number;
  fromId?: number | null;
  toId?: number | null;
}

/** What a scene leaves out. Lines outside play always stay out. */
export interface SceneFilter {
  /** Keep your prompt. */
  prompts: boolean;
  /** Keep the lines you sent. */
  commands: boolean;
  /** The channels left out, as Comm.Channel names them. */
  leftOut: string[];
}

export type SceneFormat = 'text' | 'ansi' | 'html';

/** The theme showing as you save, for the HTML file. */
export interface ScenePalette {
  background: string;
  foreground: string;
  muted: string;
  ansi: string[];
}

export interface ScenePreviewLine {
  id: number;
  ts_ms: number;
  text: string;
  raw: number[] | null;
  /** Null for a line the scene keeps, else the word shown beside it,
   *  like `prompt` or `tell`, or empty for a blank line that folds. */
  out: string | null;
}

export interface ScenePreview {
  /** The lines of the range, at most the first 5,000. */
  lines: ScenePreviewLine[];
  total: number;
  kept: number;
  /** The range holds more lines than the preview draws. */
  capped: boolean;
  /** Some lines came from a build that did not tag them, so Vosh read
   *  prompts and channels from their text. */
  older: boolean;
  /** The file's name before Downloads adds a number to one it holds. */
  file_name: string;
}

/** The lines of `range` and why the scene leaves each out. */
export async function previewScene(
  range: SceneRange,
  filter: SceneFilter,
  format: SceneFormat,
): Promise<ScenePreview> {
  return invoke('scene_preview', { range, filter, format });
}

/** Save the scene to Downloads. Resolves to the file's name. Every
 *  window hears `vosh://scene-saved` with it. */
export async function saveScene(
  range: SceneRange,
  filter: SceneFilter,
  format: SceneFormat,
  palette: ScenePalette | null,
): Promise<string> {
  return invoke('scene_save', { range, filter, format, palette });
}

/** Show a saved scene in Finder or Explorer, or its folder on Linux. */
export async function revealScene(name: string): Promise<void> {
  return invoke('scene_reveal', { name });
}

/** Hear each scene Save a scene writes, by its file's name. */
export async function subscribeSceneSaved(cb: (name: string) => void): Promise<UnlistenFn> {
  return listen<{ name: string }>(SCENE_SAVED, (event) => cb(event.payload.name));
}
