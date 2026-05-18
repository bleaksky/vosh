import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';

/// Cross-window broadcast for tracked-affect changes. The settings
/// window is a separate Tauri webview, so `window.dispatchEvent`
/// only reaches its own DOM; the main window's BottomHUD listens
/// via this Tauri channel and via the legacy window event (still
/// emitted for in-window consumers like AuxDrawer).
const TRACKED_AFFECTS_EVENT = 'vosh://tracked-affects-changed';

export async function broadcastTrackedAffects(list: string[]): Promise<void> {
  try {
    await emit(TRACKED_AFFECTS_EVENT, list);
  } catch {
    // Tauri unavailable in dev preview; the same-window CustomEvent
    // dispatched by the caller still covers the in-window path.
  }
}

export async function subscribeTrackedAffectsChanged(
  cb: (list: string[]) => void,
): Promise<UnlistenFn> {
  return listen<string[]>(TRACKED_AFFECTS_EVENT, (event) => {
    if (Array.isArray(event.payload)) cb(event.payload);
  });
}

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

export type TriggerAction =
  | { kind: 'highlight'; style: HighlightStyle }
  | { kind: 'gag' }
  | { kind: 'replace'; template: string }
  | { kind: 'send'; template: string }
  | { kind: 'route'; pane: string };

export interface TriggerRecord {
  name: string;
  pattern: string;
  priority: number;
  enabled: boolean;
  /** One or more actions. The trigger engine fires every action in
   *  order on each match. */
  actions: TriggerAction[];
  /** Set when this trigger was installed by the Highlights preset
   *  library. Toggling a preset off removes everything tagged with
   *  the preset's id; user-authored triggers leave this empty. */
  preset?: string | null;
}

/** Normalize the legacy `action: {...}` single shape that older
 *  profile.toml entries still produce on first load. Accepts either
 *  shape and returns the canonical actions array. */
export function normalizeActions(raw: unknown): TriggerAction[] {
  if (!raw || typeof raw !== 'object') return [];
  const r = raw as Record<string, unknown>;
  if (Array.isArray(r.actions)) return r.actions as TriggerAction[];
  if (r.action && typeof r.action === 'object') return [r.action as TriggerAction];
  return [];
}

export type NamedColor =
  | 'black'
  | 'red'
  | 'green'
  | 'yellow'
  | 'blue'
  | 'magenta'
  | 'cyan'
  | 'white'
  | 'bright_black'
  | 'bright_red'
  | 'bright_green'
  | 'bright_yellow'
  | 'bright_blue'
  | 'bright_magenta'
  | 'bright_cyan'
  | 'bright_white';

export interface HighlightStyle {
  fg?: NamedColor;
  bg?: NamedColor;
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

export async function exportAliases(): Promise<string> {
  return invoke('aliases_export');
}

export async function importAliases(json: string): Promise<number> {
  return invoke('aliases_import', { json });
}

export interface SystemFontEntry {
  family: string;
  monospace: boolean;
}

export async function listSystemFonts(): Promise<SystemFontEntry[]> {
  try {
    const entries = await invoke<SystemFontEntry[]>('fonts_list');
    return Array.isArray(entries) ? entries : [];
  } catch {
    return [];
  }
}

export async function presetsInstall(triggers: TriggerRecord[]): Promise<number> {
  return invoke('presets_install', { triggers });
}

export async function presetsRemove(presetId: string): Promise<number> {
  return invoke('presets_remove', { presetId });
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

export interface QuickKey {
  name: string;
  verb: string;
}

export interface TargetPayload {
  name: string | null;
  /// 1-based position in the latest Room.Chars push that the
  /// backend resolved as the targeted char. `null` when the target
  /// isn't in the current room or no target is set.
  room_idx: number | null;
  /// Current quick-key bindings (name → verb). Includes empty-verb
  /// entries; the TargetBar filters them for display.
  quick_keys: QuickKey[];
}

export async function getTarget(): Promise<TargetPayload> {
  return invoke('target_get');
}

export async function onTarget(cb: (payload: TargetPayload) => void): Promise<UnlistenFn> {
  return listen<TargetPayload>('session://target', (event) => {
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

// Keyboard macro bindings. A Macro maps a canonical key string
// (produced by canonicalKeyFromEvent below) to a command line that
// the input layer will fire when that key combo is pressed.
export interface Macro {
  key: string;
  command: string;
}

export async function listMacros(): Promise<Macro[]> {
  return invoke('macros_list');
}

export async function setMacro(key: string, command: string): Promise<Macro[]> {
  return invoke('macros_set', { key, command });
}

export async function deleteMacro(key: string): Promise<Macro[]> {
  return invoke('macros_delete', { key });
}

export async function subscribeMacrosChanged(
  cb: (macros: Macro[]) => void,
): Promise<UnlistenFn> {
  return listen<Macro[]>('vosh://macros-changed', (event) => {
    cb(event.payload);
  });
}

// Multi-format config importer. `format` is "mushclient", "mudlet",
// "gmud", or "" to auto-detect. Returns counts + the unsupported
// items the backend could not model.
export type ImportFormat = 'mushclient' | 'mudlet' | 'gmud' | '';

export interface ImportSummary {
  aliases: number;
  triggers: number;
  macros: number;
  vars: number;
  unsupported: [string, string][];
  unparsed: string[];
  rejected: string[];
}

export async function detectImportFormat(text: string): Promise<string | null> {
  return invoke('import_detect', { text });
}

export async function applyImport(
  format: ImportFormat,
  text: string,
): Promise<ImportSummary> {
  return invoke('import_apply', { format, text });
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

// ThemeChoice is now a free-form string keyed against THEMES in
// src/lib/themes.ts plus the legacy `system` sentinel for tracking the
// OS contrast preference. The settings UI populates options from the
// theme registry.
export type ThemeChoice = string;

export interface UiConfig {
  theme: ThemeChoice;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  tracked_affects: string[];
  enabled_presets: string[];
}

export async function getUiConfig(): Promise<UiConfig> {
  const cfg = await invoke<{
    theme: string;
    auto_update: boolean;
    font_family: string;
    font_size: number;
    tracked_affects: string[];
    enabled_presets: string[];
  }>('ui_get_config');
  return {
    theme: typeof cfg.theme === 'string' && cfg.theme.length > 0 ? cfg.theme : 'kanso-zen',
    auto_update: cfg.auto_update,
    font_family: cfg.font_family,
    font_size: cfg.font_size,
    tracked_affects: Array.isArray(cfg.tracked_affects) ? cfg.tracked_affects : [],
    enabled_presets: Array.isArray(cfg.enabled_presets) ? cfg.enabled_presets : [],
  };
}

export async function setUiConfig(config: UiConfig): Promise<void> {
  await invoke('ui_set_config', {
    theme: config.theme,
    autoUpdate: config.auto_update,
    fontFamily: config.font_family,
    fontSize: config.font_size,
    trackedAffects: config.tracked_affects,
    enabledPresets: config.enabled_presets,
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

export interface DockEntryPersist {
  id: string;
  zone: string;
}

export async function dockLayoutGet(): Promise<DockEntryPersist[]> {
  return invoke('dock_layout_get');
}

export async function dockLayoutSet(entries: DockEntryPersist[]): Promise<void> {
  await invoke('dock_layout_set', { entries });
}
