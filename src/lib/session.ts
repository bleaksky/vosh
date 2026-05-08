import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface OutputPayload {
  bytes: number[];
}

export type StatePayload =
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'disconnected'; reason: string | null };

export async function connectSession(host: string, port: number, tls: boolean): Promise<void> {
  await invoke('session_connect', { host, port, tls });
}

export async function disconnectSession(): Promise<void> {
  await invoke('session_disconnect');
}

export async function sendBytes(bytes: Uint8Array): Promise<void> {
  await invoke('session_send', { bytes: Array.from(bytes) });
}

export async function sendLine(line: string): Promise<void> {
  const encoder = new TextEncoder();
  const payload = encoder.encode(line + '\r\n');
  await sendBytes(payload);
}

/// Run a typed input line through the backend pipeline. Variables, aliases,
/// and slash commands are handled there; the result either goes to the
/// connection or echoes back as a session://output event.
export async function sendInput(line: string): Promise<void> {
  await invoke('session_send_input', { line });
}

export interface TriggerRecord {
  name: string;
  pattern: string;
  priority: number;
  enabled: boolean;
  action:
    | { kind: 'highlight'; style: HighlightStyle }
    | { kind: 'gag' }
    | { kind: 'replace'; template: string }
    | { kind: 'send'; template: string }
    | { kind: 'route'; pane: string };
}

export interface HighlightStyle {
  fg?: string;
  bg?: string;
  bold?: boolean;
  underline?: boolean;
  inverse?: boolean;
}

export async function exportTriggers(): Promise<string> {
  return invoke('triggers_export');
}

export async function importTriggers(json: string): Promise<number> {
  return invoke('triggers_import', { json });
}

export async function listTriggers(): Promise<TriggerRecord[]> {
  return invoke('triggers_list');
}

export interface GmcpPayload {
  package: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  data: any;
}

export async function onGmcp(cb: (payload: GmcpPayload) => void): Promise<UnlistenFn> {
  return listen<GmcpPayload>('session://gmcp', (event) => {
    cb(event.payload);
  });
}

export interface RoutedPayload {
  pane: string;
  text: string;
}

export async function onRouted(cb: (payload: RoutedPayload) => void): Promise<UnlistenFn> {
  return listen<RoutedPayload>('session://routed', (event) => {
    cb(event.payload);
  });
}

export interface TickPayload {
  enabled: boolean;
  interval_ms: number;
  remaining_ms: number;
  fired: boolean;
  sound: boolean;
}

export async function onTick(cb: (payload: TickPayload) => void): Promise<UnlistenFn> {
  return listen<TickPayload>('session://tick', (event) => {
    cb(event.payload);
  });
}

export async function exportProfile(): Promise<string> {
  return invoke('profile_export');
}

export async function importProfile(toml: string): Promise<string[]> {
  return invoke('profile_import', { toml });
}

export interface MapPayload {
  current_room_id: number | null;
  area: string | null;
}

export async function onMap(cb: (payload: MapPayload) => void): Promise<UnlistenFn> {
  return listen<MapPayload>('session://map', (event) => {
    cb(event.payload);
  });
}

export interface MapRoom {
  id: number;
  area: string;
  name: string;
  terrain: string;
  x: number;
  y: number;
  z: number;
  notes: string;
  avoid: boolean;
}

export interface MapExit {
  from_room: number;
  direction: string;
  to_room: number;
}

export interface AreaSnapshot {
  current_room_id: number | null;
  area: string;
  rooms: MapRoom[];
  exits: MapExit[];
}

export async function getAreaSnapshot(): Promise<AreaSnapshot | null> {
  return invoke('map_area_snapshot');
}

export async function walkToRoom(targetId: number): Promise<void> {
  await invoke('map_walk_to', { targetId });
}

export async function setRoomNote(roomId: number, notes: string): Promise<void> {
  await invoke('map_set_note', { roomId, notes });
}

export async function setRoomAvoid(roomId: number, avoid: boolean): Promise<void> {
  await invoke('map_set_avoid', { roomId, avoid });
}

export async function onOutput(cb: (bytes: Uint8Array) => void): Promise<UnlistenFn> {
  return listen<OutputPayload>('session://output', (event) => {
    cb(new Uint8Array(event.payload.bytes));
  });
}

export async function onState(cb: (state: StatePayload) => void): Promise<UnlistenFn> {
  return listen<StatePayload>('session://state', (event) => {
    cb(event.payload);
  });
}

export interface InputModePayload {
  password: boolean;
}

export async function onInputMode(
  cb: (payload: InputModePayload) => void,
): Promise<UnlistenFn> {
  return listen<InputModePayload>('session://input-mode', (event) => {
    cb(event.payload);
  });
}

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

export async function listLogSessions(limit: number): Promise<LogSession[]> {
  return invoke('logs_list_sessions', { limit });
}

export async function searchLogs(
  pattern: string,
  options: {
    caseSensitive?: boolean;
    maxResults?: number;
    sessionId?: number | null;
  } = {},
): Promise<LogSearchHit[]> {
  return invoke('logs_search', {
    pattern,
    caseSensitive: options.caseSensitive ?? false,
    maxResults: options.maxResults ?? 500,
    sessionId: options.sessionId ?? null,
  });
}

export async function exportLogSession(
  sessionId: number,
  withAnsi: boolean,
): Promise<string> {
  return invoke('logs_export', { sessionId, withAnsi });
}

export async function loadScrollback(): Promise<Uint8Array> {
  const bytes = await invoke<number[]>('scrollback_load');
  return new Uint8Array(bytes);
}

export type ThemeChoice = 'default' | 'high-contrast' | 'system';

export interface UiConfig {
  theme: ThemeChoice;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  tracked_affects: string[];
}

export async function getUiConfig(): Promise<UiConfig> {
  const cfg = await invoke<{
    theme: string;
    auto_update: boolean;
    font_family: string;
    font_size: number;
    tracked_affects: string[];
  }>('ui_get_config');
  const theme: ThemeChoice =
    cfg.theme === 'high-contrast' || cfg.theme === 'system' ? cfg.theme : 'default';
  return {
    theme,
    auto_update: cfg.auto_update,
    font_family: cfg.font_family,
    font_size: cfg.font_size,
    tracked_affects: Array.isArray(cfg.tracked_affects) ? cfg.tracked_affects : [],
  };
}

export async function setUiConfig(config: UiConfig): Promise<void> {
  await invoke('ui_set_config', {
    theme: config.theme,
    autoUpdate: config.auto_update,
    fontFamily: config.font_family,
    fontSize: config.font_size,
    trackedAffects: config.tracked_affects,
  });
}

export interface UpdateCheckResult {
  available: boolean;
  version: string | null;
  notes: string | null;
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  return invoke('updater_check');
}

export interface PluginInfo {
  name: string;
  version: string;
  description: string;
  author: string;
  entry: string;
  dir: string;
  enabled: boolean;
}

export async function listPlugins(): Promise<PluginInfo[]> {
  return invoke('plugins_list');
}

export async function setPluginEnabled(name: string, enabled: boolean): Promise<boolean> {
  return invoke('plugins_set_enabled', { name, enabled });
}

export async function reloadPlugin(name: string): Promise<void> {
  await invoke('plugins_reload', { name });
}
