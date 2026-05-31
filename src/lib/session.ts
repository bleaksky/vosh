import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';

/// Cross-window broadcast for tracked-affect changes. The settings
/// window is a separate Tauri webview, so `window.dispatchEvent`
/// only reaches its own DOM; the main window's BottomHUD listens
/// via this Tauri channel and via the legacy window event (still
/// emitted for in-window consumers like AuxDrawer).
const TRACKED_AFFECTS_EVENT = 'vosh://tracked-affects-changed';

/** One tracked-affect entry. `name` is what the server pushes in the
 *  Char.Affects feed (matched case-insensitively, whitespace
 *  collapsed). `label` is the optional display string shown in the
 *  affects bar — leave empty to show the name itself. Pair lets the
 *  user track "Field of Discord" but see it as "Shroud" alongside
 *  long names like "Comprehend Languages". */
export interface TrackedAffect {
  name: string;
  label: string | null;
}

export async function subscribeTrackedAffectsChanged(
  cb: (list: TrackedAffect[]) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TRACKED_AFFECTS_EVENT, (event) => {
    if (Array.isArray(event.payload)) cb(normalizeTrackedAffects(event.payload));
  });
}

/** Coerce a raw array (over-the-wire) into TrackedAffect rows.
 *  Accepts:
 *    - bare strings (legacy): "sanc" -> { name: "sanc", label: null }
 *    - full rows:             { name, label? }
 *  Empty / non-object entries are skipped. */
export function normalizeTrackedAffects(raw: unknown[]): TrackedAffect[] {
  const out: TrackedAffect[] = [];
  for (const row of raw) {
    if (typeof row === 'string') {
      const trimmed = row.trim();
      if (trimmed.length > 0) out.push({ name: trimmed, label: null });
    } else if (row && typeof row === 'object') {
      const r = row as { name?: unknown; label?: unknown };
      const name = typeof r.name === 'string' ? r.name.trim() : '';
      if (name.length === 0) continue;
      const labelRaw = typeof r.label === 'string' ? r.label.trim() : '';
      out.push({ name, label: labelRaw.length > 0 ? labelRaw : null });
    }
  }
  return out;
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

/** Push the live terminal size to the backend. The backend updates
 *  the telnet negotiator and emits a NAWS subnegotiation when NAWS
 *  has already been agreed with the server. MUDs that honor NAWS
 *  then re-wrap their output at the new column count, which is what
 *  word-wrap actually looks like: server-side wrapping at word
 *  boundaries instead of mid-character. No-op when not connected. */
export async function setWindowSize(cols: number, rows: number): Promise<void> {
  await invoke('session_set_window_size', { cols, rows });
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

/** One row inside a multi-pattern trigger. Mirrors Mudlet's
 *  per-pattern editor so a user can keep, e.g., a list of mob names
 *  as separate togglable rows instead of one long pipe regex. */
export interface TriggerPattern {
  pattern: string;
  enabled: boolean;
}

export interface TriggerRecord {
  name: string;
  /** One or more patterns. The trigger fires its actions for every
   *  enabled row that matches the line. The first row is the
   *  "primary" pattern that legacy single-pattern call sites and
   *  list summaries use. */
  patterns: TriggerPattern[];
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

/** Read a `patterns:` list out of a raw wire-shape object, falling
 *  back to the legacy `pattern:` string the backend still emits
 *  alongside for compatibility. Always returns at least one entry. */
export function normalizePatterns(raw: unknown): TriggerPattern[] {
  if (!raw || typeof raw !== 'object') return [{ pattern: '', enabled: true }];
  const r = raw as Record<string, unknown>;
  if (Array.isArray(r.patterns) && r.patterns.length > 0) {
    return r.patterns.map((row) => {
      const rr = row as Record<string, unknown>;
      return {
        pattern: String(rr.pattern ?? ''),
        enabled: rr.enabled !== false,
      };
    });
  }
  if (typeof r.pattern === 'string') {
    return [{ pattern: r.pattern, enabled: true }];
  }
  return [{ pattern: '', enabled: true }];
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

// Phase 4 perf fix: subscribe to a single GMCP package. The backend
// emits each packet on a per-package event channel
// (`session://gmcp/<package>`) so listeners run only on packets
// they care about, instead of every consumer running a string
// compare on every packet. For listeners that handle multiple
// packages (RoomStrip, useCharStats, chatStore, groupStore), call
// this once per package and manage the unsubscribes individually.
//
// Tauri event names only allow alphanumeric + `-/:_`, so dots in
// GMCP package names (`Char.Vitals`) must be encoded the same way
// the backend encodes them (`Char-Vitals`). Callers still pass the
// canonical package name with the dot; this helper rewrites it
// for the wire.
//
// The payload arrives as the package's data shape directly; the
// package name is implicit in the subscription target. Callers
// supply the data type as the generic.
//
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function onGmcpPackage<T = any>(
  name: string,
  cb: (data: T) => void,
): Promise<UnlistenFn> {
  const channel = `session://gmcp/${name.replace(/\./g, '-')}`;
  return listen<T>(channel, (event) => {
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

export async function subscribeMacrosChanged(cb: (macros: Macro[]) => void): Promise<UnlistenFn> {
  return listen<Macro[]>('vosh://macros-changed', (event) => {
    cb(event.payload);
  });
}

// Multi-format config importer. `format` is "mushclient", "mudlet",
// "gmud", or "" to auto-detect. Returns counts + the unsupported
// items the backend could not model.
export type ImportFormat = 'mushclient' | 'mudlet' | 'gmud' | 'cmud' | '';

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

export async function applyImport(format: ImportFormat, text: string): Promise<ImportSummary> {
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

export async function onInputMode(cb: (payload: InputModePayload) => void): Promise<UnlistenFn> {
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

export async function exportLogSession(sessionId: number, withAnsi: boolean): Promise<string> {
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

// User-authored theme. Mirrors AppTheme on the lib/themes side
// but with the two palette objects exposed as bare records so
// adding a slot only needs to touch themes.ts + the editor UI.
export interface CustomTheme {
  id: string;
  label: string;
  description: string;
  xterm: Record<string, string>;
  chrome: Record<string, string>;
}

export interface UiConfig {
  theme: ThemeChoice;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  tracked_affects: TrackedAffect[];
  enabled_presets: string[];
  keep_last_command: boolean;
  theme_terminal_colors: boolean;
  custom_themes: CustomTheme[];
  /** Override color for the split-scrollback divider. Empty/undefined
   *  means use the theme default (--c-border). */
  split_divider_color: string | null;
  /** When true, left/right panel zones extend the full window height
   *  and the input + status bar live only under the terminal column. */
  side_panels_fill_height: boolean;
  /** Milliseconds to wait between lines when sending a multi-line
   *  paste. 0 = no pacing; non-zero spreads sends out so the MUD
   *  flood filter does not kick. Clamped server-side to [0, 10000]. */
  paste_line_delay_ms: number;
  /** Vitals panel appearance. Toggles which columns render and lets
   *  the user pick their own bar glyphs and width. */
  vitals: VitalsConfig;
  /** Where to render the World.Moons phase glyphs in the status bar.
   *  `right-edge` is the historical placement; `before-time` and
   *  `after-time` dock the moons next to the centered tick + MUD
   *  time chip on the chosen side. */
  moons_position: MoonsPosition;
}

export type MoonsPosition = 'right-edge' | 'before-time' | 'after-time';

export type VitalsLayout = 'stacked' | 'inline';
export type VitalsPercentColor = 'fill' | 'gradient';
export type VitalsBarStyle = 'solid' | 'track';

export interface VitalsConfig {
  show_bar: boolean;
  show_percent: boolean;
  show_numeric: boolean;
  show_delta: boolean;
  bar_filled: string;
  bar_empty: string;
  bar_width: number;
  bar_style: VitalsBarStyle;
  layout: VitalsLayout;
  percent_color: VitalsPercentColor;
  template_enabled: boolean;
  template: string;
}

export const DEFAULT_VITALS_TEMPLATE =
  '%hp(%pct_hp)h %mn(%pct_mn)m %mv(%pct_mv)v - (%tick) - %time';

export const DEFAULT_VITALS_CONFIG: VitalsConfig = {
  show_bar: true,
  show_percent: true,
  show_numeric: true,
  show_delta: true,
  bar_filled: '▰',
  bar_empty: '▱',
  bar_width: 20,
  bar_style: 'solid',
  layout: 'stacked',
  percent_color: 'fill',
  template_enabled: false,
  template: DEFAULT_VITALS_TEMPLATE,
};

export async function getUiConfig(): Promise<UiConfig> {
  const cfg = await invoke<{
    theme: string;
    auto_update: boolean;
    font_family: string;
    font_size: number;
    tracked_affects: unknown[];
    enabled_presets: string[];
    keep_last_command?: boolean;
    theme_terminal_colors?: boolean;
    custom_themes?: CustomTheme[];
    split_divider_color?: string | null;
    side_panels_fill_height?: boolean;
    paste_line_delay_ms?: number;
    vitals?: Partial<VitalsConfig>;
    moons_position?: string;
  }>('ui_get_config');
  return {
    theme: typeof cfg.theme === 'string' && cfg.theme.length > 0 ? cfg.theme : 'kanso-zen',
    auto_update: cfg.auto_update,
    font_family: cfg.font_family,
    font_size: cfg.font_size,
    tracked_affects: Array.isArray(cfg.tracked_affects)
      ? normalizeTrackedAffects(cfg.tracked_affects)
      : [],
    enabled_presets: Array.isArray(cfg.enabled_presets) ? cfg.enabled_presets : [],
    keep_last_command: Boolean(cfg.keep_last_command),
    theme_terminal_colors: Boolean(cfg.theme_terminal_colors),
    custom_themes: Array.isArray(cfg.custom_themes) ? cfg.custom_themes : [],
    split_divider_color:
      typeof cfg.split_divider_color === 'string' && cfg.split_divider_color.length > 0
        ? cfg.split_divider_color
        : null,
    side_panels_fill_height: Boolean(cfg.side_panels_fill_height),
    paste_line_delay_ms:
      typeof cfg.paste_line_delay_ms === 'number' && cfg.paste_line_delay_ms >= 0
        ? Math.min(10_000, Math.floor(cfg.paste_line_delay_ms))
        : 500,
    vitals: normalizeVitalsConfig(cfg.vitals),
    moons_position:
      cfg.moons_position === 'before-time' || cfg.moons_position === 'after-time'
        ? cfg.moons_position
        : 'right-edge',
  };
}

function normalizeVitalsConfig(raw: Partial<VitalsConfig> | undefined): VitalsConfig {
  const v = raw ?? {};
  return {
    show_bar: typeof v.show_bar === 'boolean' ? v.show_bar : DEFAULT_VITALS_CONFIG.show_bar,
    show_percent:
      typeof v.show_percent === 'boolean' ? v.show_percent : DEFAULT_VITALS_CONFIG.show_percent,
    show_numeric:
      typeof v.show_numeric === 'boolean' ? v.show_numeric : DEFAULT_VITALS_CONFIG.show_numeric,
    show_delta: typeof v.show_delta === 'boolean' ? v.show_delta : DEFAULT_VITALS_CONFIG.show_delta,
    bar_filled:
      typeof v.bar_filled === 'string' && v.bar_filled.length > 0
        ? v.bar_filled
        : DEFAULT_VITALS_CONFIG.bar_filled,
    bar_empty:
      typeof v.bar_empty === 'string' && v.bar_empty.length > 0
        ? v.bar_empty
        : DEFAULT_VITALS_CONFIG.bar_empty,
    bar_width:
      typeof v.bar_width === 'number' && Number.isFinite(v.bar_width)
        ? Math.max(4, Math.min(60, Math.floor(v.bar_width)))
        : DEFAULT_VITALS_CONFIG.bar_width,
    bar_style: v.bar_style === 'track' ? 'track' : DEFAULT_VITALS_CONFIG.bar_style,
    layout: v.layout === 'inline' ? 'inline' : DEFAULT_VITALS_CONFIG.layout,
    percent_color:
      v.percent_color === 'gradient' ? 'gradient' : DEFAULT_VITALS_CONFIG.percent_color,
    template_enabled:
      typeof v.template_enabled === 'boolean'
        ? v.template_enabled
        : DEFAULT_VITALS_CONFIG.template_enabled,
    template:
      typeof v.template === 'string' && v.template.length > 0
        ? v.template
        : DEFAULT_VITALS_CONFIG.template,
  };
}

// Phase 7 perf fix: snapshot of the last UiConfig we successfully
// wrote, used to skip cross-window emits for fields the user did
// NOT change. Previously every setUiConfig call fanned out 10-11
// emits regardless of which slider moved — a font-size nudge fired
// custom-themes (500B-5KB), vitals, moons, etc. The diff cache cuts
// that to "emit only for fields that actually moved" without
// changing wire-protocol or subscriber surface.
let lastSentConfig: UiConfig | null = null;

async function emitChanged<T>(
  event: string,
  value: T,
  prevValue: T | undefined,
  equal: (a: T, b: T) => boolean = Object.is,
): Promise<void> {
  if (prevValue !== undefined && equal(value, prevValue)) return;
  try {
    await emit(event, value);
  } catch {
    // Tauri bus unavailable (dev preview, window not yet ready); the
    // same-window in-process state is already updated by the caller.
  }
}

// Cheap deep-equality for the structured fields. Both vitals and
// custom_themes are small bounded objects, so JSON round-trip is
// faster (and more predictable) than a hand-rolled walker.
function deepEqual<T>(a: T, b: T): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export async function setUiConfig(config: UiConfig): Promise<void> {
  await invoke('ui_set_config', {
    theme: config.theme,
    autoUpdate: config.auto_update,
    fontFamily: config.font_family,
    fontSize: config.font_size,
    // Wire format intentionally drops `label: null` to the omitted
    // form so the backend's `Option<String>` deserializes cleanly.
    trackedAffects: config.tracked_affects.map((t) => ({
      name: t.name,
      ...(t.label ? { label: t.label } : {}),
    })),
    enabledPresets: config.enabled_presets,
    keepLastCommand: config.keep_last_command,
    themeTerminalColors: config.theme_terminal_colors,
    customThemes: config.custom_themes,
    splitDividerColor: config.split_divider_color,
    sidePanelsFillHeight: config.side_panels_fill_height,
    pasteLineDelayMs: config.paste_line_delay_ms,
    vitals: config.vitals,
    moonsPosition: config.moons_position,
  });
  const prev = lastSentConfig;
  // Snapshot BEFORE the emits so a listener that re-reads via
  // `getUiConfig` sees the new state if it races us.
  lastSentConfig = config;

  // Emit theme + custom-themes BEFORE any other event so the main
  // window's theme registry is up-to-date by the time `theme-changed`
  // points at a custom theme id. The ordering matters; the diffing
  // does not change that.
  await emitChanged(
    'vosh://custom-themes-changed',
    config.custom_themes,
    prev?.custom_themes,
    deepEqual,
  );
  await emitChanged('vosh://theme-changed', config.theme, prev?.theme);
  await emitChanged(
    'vosh://font-changed',
    { family: config.font_family, size: config.font_size },
    prev ? { family: prev.font_family, size: prev.font_size } : undefined,
    (a, b) => a.family === b.family && a.size === b.size,
  );
  await emitChanged('vosh://keep-last-changed', config.keep_last_command, prev?.keep_last_command);
  await emitChanged(
    'vosh://theme-terminal-colors-changed',
    config.theme_terminal_colors,
    prev?.theme_terminal_colors,
  );
  await emitChanged(
    'vosh://split-divider-changed',
    config.split_divider_color,
    prev?.split_divider_color,
  );
  await emitChanged(
    'vosh://side-panels-fill-height-changed',
    config.side_panels_fill_height,
    prev?.side_panels_fill_height,
  );
  await emitChanged(
    'vosh://paste-line-delay-changed',
    config.paste_line_delay_ms,
    prev?.paste_line_delay_ms,
  );
  await emitChanged('vosh://vitals-config-changed', config.vitals, prev?.vitals, deepEqual);
  await emitChanged('vosh://moons-position-changed', config.moons_position, prev?.moons_position);
  await emitChanged(
    TRACKED_AFFECTS_EVENT,
    config.tracked_affects,
    prev?.tracked_affects,
    deepEqual,
  );
}

export async function subscribeMoonsPositionChanged(
  cb: (value: MoonsPosition) => void,
): Promise<UnlistenFn> {
  return listen<string>('vosh://moons-position-changed', (event) => {
    const v = event.payload;
    cb(v === 'before-time' || v === 'after-time' ? v : 'right-edge');
  });
}

export async function subscribeVitalsConfigChanged(
  cb: (config: VitalsConfig) => void,
): Promise<UnlistenFn> {
  return listen<Partial<VitalsConfig>>('vosh://vitals-config-changed', (event) => {
    cb(normalizeVitalsConfig(event.payload));
  });
}

export async function subscribeSidePanelsFillHeightChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>('vosh://side-panels-fill-height-changed', (event) => {
    cb(Boolean(event.payload));
  });
}

export async function subscribeSplitDividerChanged(
  cb: (color: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>('vosh://split-divider-changed', (event) => {
    cb(typeof event.payload === 'string' && event.payload.length > 0 ? event.payload : null);
  });
}

export async function subscribeCustomThemesChanged(
  cb: (themes: CustomTheme[]) => void,
): Promise<UnlistenFn> {
  return listen<CustomTheme[]>('vosh://custom-themes-changed', (event) => {
    cb(Array.isArray(event.payload) ? event.payload : []);
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

// Named profile catalog.
export interface ProfileAutoMatch {
  host?: string | null;
  port?: number | null;
  character?: string | null;
}

export interface ProfileEntry {
  name: string;
  description?: string | null;
  auto_match?: ProfileAutoMatch | null;
}

export interface ProfilesList {
  active: string;
  profiles: ProfileEntry[];
}

export async function profilesList(): Promise<ProfilesList> {
  return invoke('profiles_list');
}

export async function profileCreate(name: string): Promise<void> {
  return invoke('profile_create', { name });
}

export async function profileDelete(name: string): Promise<void> {
  return invoke('profile_delete', { name });
}

export async function profileRename(oldName: string, newName: string): Promise<void> {
  return invoke('profile_rename', { old: oldName, new: newName });
}

export async function profileDuplicate(source: string, newName: string): Promise<void> {
  return invoke('profile_duplicate', { source, new: newName });
}

export async function profileSwitch(name: string): Promise<void> {
  return invoke('profile_switch', { name });
}

export async function profileSetMetadata(
  name: string,
  description: string | null,
  autoMatch: ProfileAutoMatch | null,
): Promise<void> {
  return invoke('profile_set_metadata', {
    name,
    description,
    autoMatch,
  });
}

export async function profileResolveMatch(
  host: string,
  port: number,
  character: string | null,
): Promise<string | null> {
  return invoke('profile_resolve_match', { host, port, character });
}

// Per-category scope toggle (Profile vs Global).
export type ProfileScope = 'profile' | 'global';

export interface ScopeConfig {
  theme: ProfileScope;
  font: ProfileScope;
  dock_layout: ProfileScope;
  keep_last_command: ProfileScope;
  auto_update: ProfileScope;
}

export async function profileGetScope(): Promise<ScopeConfig> {
  return invoke('profile_get_scope');
}

export async function profileSetScope(scope: ScopeConfig): Promise<void> {
  return invoke('profile_set_scope', { scope });
}

export async function subscribeProfilesChanged(
  cb: (changedName: string) => void,
): Promise<UnlistenFn> {
  return listen<string>('vosh://profiles-changed', (event) => {
    cb(event.payload);
  });
}

export async function subscribeProfileSwitched(
  cb: (newActive: string) => void,
): Promise<UnlistenFn> {
  return listen<string>('vosh://profile-switched', (event) => {
    cb(event.payload);
  });
}

export async function installUpdateAndRelaunch(): Promise<void> {
  return invoke('updater_install_and_relaunch');
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
  /** Vertical alignment within a left/right zone: 'top' or 'bottom'.
   *  Missing means top. Ignored for full-width zones (top/bottom/hidden). */
  align?: string;
}

export async function dockLayoutGet(): Promise<DockEntryPersist[]> {
  return invoke('dock_layout_get');
}

export async function dockLayoutSet(entries: DockEntryPersist[]): Promise<void> {
  await invoke('dock_layout_set', { entries });
}

export async function subscribeDockLayoutChanged(
  cb: (entries: DockEntryPersist[]) => void,
): Promise<UnlistenFn> {
  return listen<DockEntryPersist[]>('vosh://dock-layout-changed', (event) => {
    cb(Array.isArray(event.payload) ? event.payload : []);
  });
}
