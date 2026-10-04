import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { CRITICAL_TICKS, EXPIRING_TICKS } from './affectsView';
import { sanitizeLayout, type PaneLayout } from './paneLayout';
import {
  resolveActiveTheme,
  systemPrefersDark,
  THEME_PREFS_EVENT,
  themePrefsOf,
  type ThemePrefs,
} from './theme';
import { uniqueThemeId } from './themeImport';
import { BUILTIN_THEMES, customToAppTheme, DEFAULT_THEME_ID, themeTokens } from './themes';

/** Resolve the tri-state tint setting: an explicit user choice wins;
 *  unset is on for every theme. The chrome derives its status colors
 *  from the theme's ANSI slots, so output painted in the same slots
 *  keeps the MUD's red and the chrome's red in agreement. */
export function resolveThemeTerminalColors(_theme: string, stored: boolean | null): boolean {
  return stored ?? true;
}

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
  /** Output bytes as standard base64. Decoded once with `atob` in
   *  `onOutput`. Replaces the old `number[]` array, which made the
   *  backend serialize one JSON number per byte. */
  b64: string;
  /** Replace a region an earlier payload marked, applied before `b64`
   *  (see src/lib/terminalRegion.ts). */
  replace?: {
    gen: number;
    b64: string;
    fresh: boolean;
    /** The lines the region's prompt shows right above it, as plain
     *  text, and what goes in their place when they are there. */
    above?: { plain: string; b64: string };
    /** The end of the region the bytes leave out, as base64, which a
     *  terminal that writes them on a new row, or finds the region open
     *  with nothing held back, holds back in their place. */
    tail?: string;
  };
  /** The live render for the region this payload leaves open, as
   *  base64, written back before anything else lands. */
  restore?: string;
  /** What the band above the command line shows from now on, as base64,
   *  while your prompt shows pinned. An empty string clears it. */
  pin?: string;
  /** Where each piece of your design landed on the band `pin` shows,
   *  rows counted from the band's first. Absent when it shows no design. */
  pin_spans?: PromptSpan[];
  /** Line ends at the end of this payload that each renderer keeps back
   *  until the next write lands, as base64. */
  hold?: string;
  /** While your prompt shows pinned, whether the pinned prompt's row is
   *  still where the next thing lands after this payload, so the line
   *  end that would end that row writes nothing. */
  pin_row?: boolean;
  /** Which output of the prompt stage this is. Absent on output from
   *  elsewhere, such as a slash command's echo. */
  id?: number;
}

/** One session write, decoded: the replace goes first, then `bytes`,
 *  and `hold` waits for the next write. */
export interface SessionOutput {
  bytes: Uint8Array;
  replace?: {
    gen: number;
    bytes: Uint8Array;
    fresh: boolean;
    above?: { plain: string; bytes: Uint8Array };
    tail?: Uint8Array;
  };
  restore?: Uint8Array;
  pin?: Uint8Array;
  /** Where each piece of your design landed on the band `pin` shows. */
  pinSpans?: PromptSpan[];
  hold?: Uint8Array;
  pinRow?: boolean;
  /** Which output of the prompt stage this is. A terminal keeps the
   *  newest it took, so text it writes itself can tell the session
   *  which output it follows. */
  id?: number;
}

/** Standard base64 to bytes. `atob` yields a binary string, one char per
 *  byte, and each char code goes back to a byte. Far cheaper than parsing
 *  a JSON number[] and copying it. */
function base64Bytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

/** Decode a `session://output` payload. */
export function decodeOutputPayload(payload: OutputPayload): SessionOutput {
  const out: SessionOutput = { bytes: base64Bytes(payload.b64) };
  const replace = payload.replace;
  if (replace) {
    out.replace = { gen: replace.gen, bytes: base64Bytes(replace.b64), fresh: replace.fresh };
    if (replace.above) {
      out.replace.above = { plain: replace.above.plain, bytes: base64Bytes(replace.above.b64) };
    }
    if (typeof replace.tail === 'string') out.replace.tail = base64Bytes(replace.tail);
  }
  if (typeof payload.restore === 'string') out.restore = base64Bytes(payload.restore);
  if (typeof payload.pin === 'string') out.pin = base64Bytes(payload.pin);
  if (Array.isArray(payload.pin_spans)) out.pinSpans = payload.pin_spans;
  if (typeof payload.hold === 'string') out.hold = base64Bytes(payload.hold);
  if (typeof payload.pin_row === 'boolean') out.pinRow = payload.pin_row;
  if (typeof payload.id === 'number') out.id = payload.id;
  return out;
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

/// Run a typed input line through the backend pipeline. Variables, aliases,
/// and slash commands are handled there; the result either goes to the
/// connection or echoes back as a session://output event.
export async function sendInput(line: string): Promise<void> {
  await invoke('session_send_input', { line });
}

/// Send a line typed into the masked password field. It goes to the
/// server exactly as typed, past aliases, variables, and slash commands,
/// and the session log keeps `> (hidden)` in its place.
export async function sendMaskedInput(line: string): Promise<void> {
  await invoke('session_send_masked', { line });
}

/// Stop the walk under way, as Esc in the command line does. The session
/// says nothing when you are not walking.
export async function stopWalk(): Promise<void> {
  await invoke('session_walk_stop');
}

export type TriggerAction =
  | { kind: 'highlight'; style: HighlightStyle }
  | { kind: 'gag' }
  | { kind: 'replace'; template: string }
  | { kind: 'send'; template: string }
  | { kind: 'route'; pane: string }
  | { kind: 'script'; body: string };

/** One row inside a multi-pattern trigger. Mirrors Mudlet's
 *  per-pattern editor so a user can keep, e.g., a list of mob names
 *  as separate togglable rows instead of one long pipe regex. */
export interface TriggerPattern {
  /** The regex of a Regex row. A Text or Starts with row from the store
   *  holds the regex its text compiles to, which builds up to 0.8.1
   *  read, and the store reads `text` in its place. An edit changes only
   *  `text`, and the store writes the new regex here on Save. Read and
   *  edit what you typed through `patternSource` and `withPatternSource`. */
  pattern: string;
  enabled: boolean;
  /** How the store reads the row. Left out, and on the wire, while it
   *  is 'regex'. See `MatchMode` in crates/automation/src/trigger/store.rs. */
  mode?: MatchMode;
  /** What you typed in a Text or Starts with row. A row with one of
   *  those modes and no text, as builds before the field saved it,
   *  reads `pattern` as the text, and normalizePatterns copies it here.
   *  A Regex row has none. */
  text?: string;
}

/** 'text' matches a line that is exactly the pattern, with spaces at
 *  either end of the line and of the pattern skipped. 'starts_with'
 *  matches a line that starts with it, after any spaces at the start of
 *  either, and its match runs to the end of the line. Neither has
 *  groups. 'regex' reads the pattern as typed. */
export type MatchMode = 'text' | 'starts_with' | 'regex';

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
  /** Optional user-defined group / folder. Triggers sharing a group
   *  can be bulk-toggled via the per-group switch in the Settings
   *  Triggers tab without losing their individual `enabled` flags.
   *  Undefined / null means ungrouped. */
  group?: string | null;
  /** Which dispatch lane the trigger runs in. 'line' (default) fires
   *  on every completed line of server output. 'prompt' fires on
   *  the partial-prompt buffer the telnet parser flushes on GA/EOR
   *  so #prompt-style triggers can capture from prompt text that
   *  arrives without a trailing newline. 'room' fires only on the
   *  lines a room look lists after its exits line, the armies, the
   *  things and the people in the room (src-tauri/src/session/room_block.rs).
   *  'room_target' fires only on the line of the person you target with
   *  `tar` among them. Omitted on the wire when the value is 'line' (the
   *  backend's default). */
  target?: TriggerTarget;
}

export type TriggerTarget = 'line' | 'prompt' | 'room' | 'room_target';

/** Read a `patterns:` list out of a raw wire-shape object, falling
 *  back to the legacy `pattern:` string the backend still emits
 *  alongside for compatibility. Always returns at least one entry. */
export function normalizePatterns(raw: unknown): TriggerPattern[] {
  if (!raw || typeof raw !== 'object') return [{ pattern: '', enabled: true }];
  const r = raw as Record<string, unknown>;
  if (Array.isArray(r.patterns) && r.patterns.length > 0) {
    return r.patterns.map((row) => {
      const rr = (row && typeof row === 'object' ? row : {}) as Record<string, unknown>;
      const out: TriggerPattern = {
        pattern: String(rr.pattern ?? ''),
        enabled: rr.enabled !== false,
      };
      // A save sends the row back as the page holds it, so the mode has
      // to ride along or the store reads the text as a regex. So does the
      // text, or the store reads the regex in `pattern` as the text. A
      // row with no text, as builds before the field saved it, takes
      // `pattern` as its text, so every such row on the page holds one
      // and a row you edit and type back matches its saved copy.
      if (rr.mode === 'text' || rr.mode === 'starts_with') {
        out.mode = rr.mode;
        out.text = typeof rr.text === 'string' ? rr.text : out.pattern;
      }
      return out;
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
  /** Full-line wash: the whole line gets a dim background tint derived
   *  from the highlight color, plus a left-edge accent bar in the
   *  native renderer. */
  wash?: boolean;
  /** Base color: the style fills only the text the game left in its
   *  default color, so the colors the game puts on parts of the line
   *  stay, and other highlights draw over it. No span and no wash. */
  base?: boolean;
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
// packages (roomStore, chatStore, groupStore), call
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

/** The last Char.Affects payload of this connection, raw as the MUD
 *  sent it, or null. A window that opens between ticks reads it so it
 *  shows the affects on you without waiting for the next list. */
export async function affectsSnapshotGet(): Promise<unknown> {
  return invoke('affects_snapshot_get');
}

/** The affect fulls the backend keeps for the logged in character. */
export async function affectFullGet(): Promise<unknown> {
  return invoke('affect_full_get');
}

/** Hear the affect fulls change: a list that starts, recasts, or ends
 *  an affect, the saved fulls at login, or a disconnect that empties
 *  them. The payload is the whole map. */
export async function subscribeAffectFullChanged(
  cb: (value: unknown) => void,
): Promise<UnlistenFn> {
  return listen<unknown>('vosh://affect-full-changed', (event) => cb(event.payload));
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

/** The tick timer as the session loop reports it on session://tick,
 *  four times a second and on every tick. */
export interface TickPayload {
  enabled: boolean;
  interval_ms: number;
  /** Time left until the expected tick, 0 once it has passed. */
  remaining_ms: number;
  /** Time since the last tick. It keeps growing past the interval
   *  while the game runs late. */
  elapsed_ms: number;
  /** The expected tick has come and the game's tick has not. */
  overdue: boolean;
  /** The game's own tick decides when the timer fires. */
  synced: boolean;
  /** This report is the tick itself, so the sound plays once. */
  fired: boolean;
  sound: boolean;
}

export async function onTick(cb: (payload: TickPayload) => void): Promise<UnlistenFn> {
  return listen<TickPayload>('session://tick', (event) => {
    cb(event.payload);
  });
}

// Prompt vars, the values a trigger writes with
// `mud.set_prompt_var(...)`. The vitals store reads them with priority
// over GMCP, so a #prompt capture can feed hp, mana and moves from the
// prompt text. The payload is the full snapshot, and the frontend
// replaces its copy. A value for a name GMCP also supplies, such as
// hp, drops out at the next Char.Vitals, or at your next send on a
// server without it. A value the game hides comes as `?`.
export type PromptVarsPayload = Record<string, string>;

export async function onPromptVars(cb: (payload: PromptVarsPayload) => void): Promise<UnlistenFn> {
  return listen<PromptVarsPayload>('session://prompt-vars', (event) => {
    cb(event.payload);
  });
}

/** Which values the game hides right now, on session://hidden. The
 *  prompt engine works it out from the latest packets and your prompt
 *  on every server build, and sends it once per read when it changes.
 *  `vitals` covers your health, mana and moves, `tank` the health of
 *  the groupmate your opponent hits, and `opponent` your opponent's
 *  health and condition. */
export interface HiddenPayload {
  vitals: boolean;
  tank: boolean;
  opponent: boolean;
  affects: boolean;
  group: boolean;
}

export async function onHidden(cb: (payload: HiddenPayload) => void): Promise<UnlistenFn> {
  return listen<HiddenPayload>('session://hidden', (event) => {
    cb(event.payload);
  });
}

/** The game told Vosh your prompt settings, on
 *  session://game-prompt-seen. `kind` is `gmcp` for Char.Prompt,
 *  `prompt` or `fprompt` for a line that shows a setting, and `off` when
 *  you turned prompts off. `text` is the setting as the game stores it.
 *  `applied` says the active profile's capture took it. */
export interface GamePromptSeenPayload {
  kind: 'gmcp' | 'prompt' | 'fprompt' | 'off';
  text: string;
  applied: boolean;
  /** The catalog names of the parts of your design the capture fed
   *  before and nothing feeds now. */
  lost: string[];
}

export async function onGamePromptSeen(
  cb: (payload: GamePromptSeenPayload) => void,
): Promise<UnlistenFn> {
  return listen<GamePromptSeenPayload>('session://game-prompt-seen', (event) => {
    const lost = (event.payload as { lost?: unknown }).lost;
    cb({
      ...event.payload,
      lost: Array.isArray(lost) ? lost.filter((n): n is string => typeof n === 'string') : [],
    });
  });
}

/** Your prompt settings and where Vosh last saw them: `gmcp` for the
 *  latest Char.Prompt, `session` for the game's reply to your own
 *  `prompt` this session, `log` for the newest such reply in your log
 *  that belongs to one of this profile's characters. `at` is RFC 3339
 *  local time. */
export interface PromptLastSeen {
  prompt: string | null;
  fprompt: string | null;
  enabled: boolean | null;
  at: string | null;
  at_login: boolean;
  source: 'gmcp' | 'session' | 'log';
  character: string | null;
}

/** Where Vosh last saw your prompt settings, or null when it has not. */
export async function promptLastSeen(): Promise<PromptLastSeen | null> {
  return invoke('prompt_last_seen');
}

/** What the backend last reported on session://hidden, for a window
 *  that opens or reloads after the report. */
export async function hiddenGet(): Promise<HiddenPayload> {
  return invoke('hidden_get');
}

// Prompt
//
// The prompt editor's commands and events (section 6 of the prompt
// editor build spec). The backend owns every byte decision: the card
// reads the [prompt] table, asks what a capture compiles to, draws
// designs through prompt_render, and writes template text only through
// prompt_edit.

/** Where a capture came from. */
export type PromptCaptureSource = 'gmcp' | 'session' | 'log' | 'typed' | 'migrated';

/** `[prompt.capture]`, how Vosh reads the game's prompt. */
export type PromptCapture =
  | { kind: 'none' }
  | {
      kind: 'aabahran';
      prompt: string;
      fprompt: string;
      follow_game: boolean;
      seen_at?: string | null;
      source?: PromptCaptureSource | null;
    }
  | {
      kind: 'regex';
      lines: string[];
      settle: boolean;
      /** The variable each group feeds, where it differs from the group. */
      names?: Record<string, string>;
      seen_at?: string | null;
      source?: PromptCaptureSource | null;
    };

/** The active profile's `[prompt]` table. */
export interface PromptConfig {
  draw: boolean;
  template: string;
  /** At most two designs the card opened with, newest first. */
  previous_templates: string[];
  capture: PromptCapture;
  show: PromptShow;
  /** The design follows the game. The backend writes it from your PROMPT
   *  and fight prompt codes, as Same as the game, each time they change.
   *  Any edit makes the design yours, and picking Same as the game in
   *  the start list follows the game again. */
  mirror: boolean;
}

interface RawPromptConfig {
  draw?: unknown;
  template?: unknown;
  previous_templates?: unknown;
  capture?: unknown;
  show?: unknown;
  mirror?: unknown;
}

/** A table from what the backend sent. It leaves out an empty list of
 *  earlier designs, no capture, the text, and a design of yours, so
 *  those come back as the defaults. */
export function normalizePromptConfig(raw: RawPromptConfig | null): PromptConfig {
  const capture = raw?.capture as PromptCapture | undefined;
  const kinds = ['none', 'aabahran', 'regex'];
  return {
    draw: raw?.draw === true,
    template: typeof raw?.template === 'string' ? raw.template : '',
    previous_templates: Array.isArray(raw?.previous_templates)
      ? raw.previous_templates.filter((t): t is string => typeof t === 'string')
      : [],
    capture:
      capture && typeof capture === 'object' && kinds.includes(capture.kind)
        ? capture
        : { kind: 'none' },
    show: normalizePromptShow(raw?.show),
    mirror: raw?.mirror === true,
  };
}

/** The active profile's `[prompt]` table. */
export async function promptConfigGet(): Promise<PromptConfig> {
  return normalizePromptConfig(await invoke<RawPromptConfig | null>('prompt_config_get'));
}

/** Tell the backend the card opened, which keeps the design it found
 *  among the earlier designs, and read the table as it now stands. */
export async function promptCardOpen(): Promise<PromptConfig> {
  return normalizePromptConfig(await invoke<RawPromptConfig | null>('prompt_card_open'));
}

/** What the main window opens the card on: where it would open, Edit as
 *  text, or pointing at the line your game prints, which Point at it
 *  again… in Settings asks for. */
export type PromptCardView = 'text' | 'point' | null;

export interface PromptCardRequest {
  view: PromptCardView;
}

export const PROMPT_CARD_OPEN_EVENT = 'vosh://prompt-card-open';

/** Ask the main window to open the prompt card, from any window, such as
 *  Customize… in Settings. */
export async function openPromptCard(view: PromptCardView = null): Promise<void> {
  await emit(PROMPT_CARD_OPEN_EVENT, { view });
}

/** Hear a window ask for the prompt card. */
export async function subscribePromptCardOpen(
  cb: (request: PromptCardRequest) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PROMPT_CARD_OPEN_EVENT, (event) => {
    const raw = event.payload as { view?: unknown } | null;
    const view = raw?.view;
    cb({ view: view === 'text' || view === 'point' ? view : null });
  });
}

/** Save a `[prompt]` table for the active profile. It saves shortly,
 *  repaints the open row and tells every window. A capture that does not
 *  compile changes nothing, and the error is a sentence to show. Turning
 *  drawing on with no design follows the game, unless `asIs` keeps the
 *  design exactly as sent, as Start empty does. A table that follows the
 *  game takes the design written from its codes. */
export async function promptConfigSet(
  config: PromptConfig,
  options?: { asIs?: boolean },
): Promise<void> {
  await invoke('prompt_config_set', options?.asIs ? { config, asIs: true } : { config });
}

/** A design another profile holds. */
export interface PromptDesign {
  profile: string;
  display_name: string;
  template: string;
}

/** The designs every other profile holds, for From another profile. */
export async function promptDesignsList(): Promise<PromptDesign[]> {
  return invoke('prompt_designs_list');
}

/** What `prompt_compile` reads. `typed` says you typed or pasted the
 *  setting as you type it in the game, so Vosh stores it as the game
 *  would first. */
export type PromptCompileRequest =
  | { kind: 'aabahran'; prompt: string; fprompt?: string; typed?: boolean }
  | { kind: 'regex'; lines: string[]; names?: Record<string, string> };

/** Which of your two settings a code or a warning is from. */
export type PromptWhich = 'prompt' | 'fprompt';

export type PromptWarningKind =
  | 'run_together'
  | 'short'
  | 'pacify_mortal'
  | 'lang_mobile'
  | 'cut'
  | 'lone_percent'
  | 'twice';

export type PromptPresetId =
  | 'default'
  | 'game'
  | 'minimal'
  | 'how_full'
  | 'percent'
  | 'bars'
  | 'detailed'
  | 'empty';

/** A design to start from. */
export interface PromptPreset {
  id: PromptPresetId;
  label: string;
  template: string;
}

/** One number of a line you pointed at, as the card marks it. `span` is
 *  a byte range in the line, `name` the value it reads into, empty when
 *  you left it out, and `suggested` the name Vosh read from the letters
 *  after it. `max` marks the second number of a pair like `100/120hp`. */
export interface PromptLineNumber {
  span: [number, number];
  text: string;
  name: string;
  suggested: string;
  label: string;
  max: boolean;
}

/** What a capture compiles to. Spans are byte ranges in the setting as
 *  the game stores it. */
export interface PromptCompileReport {
  ok: boolean;
  error: { message: string; which: PromptWhich | null; span: [number, number] | null } | null;
  prompt: string;
  fprompt: string;
  shapes: {
    lines: string[];
    settle: boolean;
  }[];
  warnings: {
    kind: PromptWarningKind;
    which: PromptWhich;
    span: [number, number];
    message: string;
  }[];
  presets: PromptPreset[];
  /** The value each group of a pattern feeds where it differs from the
   *  group's own name, as the capture's `names` keeps it. */
  names: Record<string, string>;
  /** Each number of a line you pointed at. Empty otherwise. */
  numbers: PromptLineNumber[];
  /** The card's code legend: every code and line end in the order the
   *  settings print them. Empty for a pattern. */
  legend: PromptLegendRow[];
  /** What the prompt shows, as the card says it, or null when codes run
   *  together or it reads no value. */
  shows: string | null;
  /** While codes run together, what Vosh still reads and which values
   *  the game supplies until you fix the prompt. */
  fix_note: string | null;
  /** While codes run together, the command that fixes each setting. */
  fixes: string[];
  /** For a line another game prints, the values its GMCP sends that
   *  Vosh has no name for. */
  gmcp_names: { name: string; package: string }[];
}

/** One row of the card's code legend. */
export interface PromptLegendRow {
  /** As you write it, `%h`, or a run of codes that run together. Empty
   *  for a warning about the whole setting. */
  code: string;
  label: string;
  which: PromptWhich;
  span: [number, number];
  /** It prints only in a fight, which the card tags while the prompt it
   *  shows is not from one. */
  fight: boolean;
  tag: string | null;
  /** The code carries the warn ring. */
  warn: boolean;
  /** The warning's sentence, shown under the row. */
  warning: string | null;
}

/** What a capture compiles to. It changes nothing. */
export async function promptCompile(capture: PromptCompileRequest): Promise<PromptCompileReport> {
  return invoke('prompt_compile', { capture });
}

/** One entry of the candidates ring: what came right before a send or a
 *  GA. `raw` keeps the game's colors. */
export interface PromptCandidate {
  id: number;
  raw: string;
  plain: string;
  at_ms: number;
  recognized: boolean;
  draw: boolean;
  capture: boolean;
}

/** Ring entries that share a shape, digits masked as `#`, newest first. */
export interface PromptCandidateGroup {
  shape: string;
  count: number;
  recognized: boolean;
  entries: PromptCandidate[];
}

/** The candidates ring grouped by shape, the largest group first. */
export async function promptCandidates(): Promise<PromptCandidateGroup[]> {
  return invoke('prompt_candidates');
}

/** How a capture matches the ring and your scrollback, with the match
 *  line the card shows. */
export interface PromptCaptureCheck {
  matched: number;
  total: number;
  fight_matched: number;
  false_matches: number;
  text: string;
  /** Each ring entry the capture reads, newest first, with its values
   *  marked. */
  reads: PromptCheckRead[];
}

/** What one value printed in a prompt: its line, top line first, and its
 *  characters in that line, counted by code point. `field` is null for
 *  codes that run together, which `warn` marks. */
export interface PromptMark {
  line: number;
  start: number;
  end: number;
  field: string | null;
  label: string;
  warn: boolean;
}

/** A ring entry a capture reads. */
export interface PromptCheckRead {
  id: number;
  /** As the game sent it, colors included, lines joined by `\r\n`. */
  raw: string;
  /** Lines joined by `\n`. */
  plain: string;
  at_ms: number;
  fight: boolean;
  marks: PromptMark[];
}

/** What a capture built from one ring entry reads, the line another game
 *  prints before each command. `names` names its numbers in order, an
 *  empty name leaves one out, and the rest take the names Vosh suggests.
 *  The report's shape holds the pattern to save. */
export async function promptCaptureFromLine(
  id: number,
  names?: string[],
): Promise<PromptCompileReport> {
  return invoke('prompt_capture_from_line', { id, names: names ?? null });
}

/** Check a capture against the candidates ring and your scrollback. */
export async function promptCaptureCheck(capture: PromptCapture): Promise<PromptCaptureCheck> {
  return invoke('prompt_capture_check', { capture });
}

/** A color in a span. Palette indexes resolve through the active theme. */
export type PromptSpanColor =
  | { kind: 'default' }
  | { kind: 'index'; index: number }
  | { kind: 'rgb'; r: number; g: number; b: number };

/** Where a piece of a design landed: `row` is the line from `%nl`, `col`
 *  the cell in it before any wrap, and `width` the cells it takes, a wide
 *  character two and a combining mark none (`cellWidth` in sgrCells.ts).
 *  The look is the one at the piece's first cell. */
export interface PromptSpan {
  piece: number;
  row: number;
  col: number;
  width: number;
  fg: PromptSpanColor;
  bg: PromptSpanColor;
  bold: boolean;
  italic: boolean;
  underline: boolean;
}

/** A drawn design. */
export interface PromptRendered {
  ansi: string;
  plain: string;
  rows: number;
  spans: PromptSpan[];
}

/** Values a preview draws in place of the live ones, by field in the
 *  form the samples take. `"?"` draws a value hidden and `null` absent.
 *  `lament` hides every value the song hides. */
export interface PromptOverrides {
  values?: Record<string, string | number | boolean | null>;
  lament?: boolean;
}

/** The previews the card's footer offers. */
export type PromptPreviewName = 'now' | 'low_health' | 'fight' | 'lament';

/** One design to draw, with live or sample values. `preview` draws one
 *  of the card's previews, and `overrides` go on top of it. */
export interface PromptRenderRequest {
  template: string;
  values?: 'live' | 'sample';
  preview?: PromptPreviewName | null;
  overrides?: PromptOverrides | null;
  /** Draw each value with nothing to show as its label, as the open
   *  card does. */
  placeholders?: boolean;
}

/** Draw a design. */
export async function promptRender(request: PromptRenderRequest): Promise<PromptRendered> {
  return invoke('prompt_render', {
    template: request.template,
    values: request.values ?? 'live',
    preview: request.preview ?? null,
    overrides: request.overrides ?? null,
    placeholders: request.placeholders ?? false,
  });
}

/** Draw several designs at once, such as the start list. */
export async function promptRenderMany(requests: PromptRenderRequest[]): Promise<PromptRendered[]> {
  return invoke('prompt_render_many', { requests });
}

/** What the open card shows on your prompt in place of the live render:
 *  one of its previews with values on top, the labels of values with
 *  nothing to show (`placeholders`), or the line the game sent while it
 *  reads your codes (`raw`). */
export interface PromptPreview {
  preview?: PromptPreviewName | null;
  overrides?: PromptOverrides | null;
  placeholders?: boolean;
  raw?: boolean;
}

/** Show a preview on your prompt, or the live render again with null.
 *  The open row carries the live render as its restore, so only live
 *  renders reach history. Nothing saves or goes to the game. */
export async function promptPreviewSet(preview: PromptPreview | null): Promise<void> {
  await invoke('prompt_preview_set', { preview });
}

/** The open card chose Aabahran's code reader on a host Vosh does not
 *  know (More > Use Forsaken Lands prompt codes…), or lets it go. While
 *  it holds, the Forsaken Lands rules hold, so the game's reply to prompt
 *  fills the card's fields on an older build (D17). */
export async function promptCodeReaderSet(on: boolean): Promise<void> {
  await invoke('prompt_code_reader_set', { on });
}

export type PromptFormatName =
  | 'value'
  | 'cur_max'
  | 'max'
  | 'pct'
  | 'pct_game'
  | 'percent'
  | 'bar'
  | 'game'
  | 'word'
  | 'ampm'
  | 'name'
  | 'grouped'
  | 'short'
  | 'thousands'
  | 'unit'
  | 'since'
  | 'trunc'
  | 'hm'
  | 'hms'
  | 'md'
  | 'count'
  | 'names'
  | 'on'
  | 'off';

/** A color the card offers. By value with no field is the piece's own
 *  value, by how full it is, by the game's `%h` bands with `game`, or in
 *  eleven steps from red to green with `steps`. */
export type PromptColorChoice =
  | { kind: 'default' }
  | { kind: 'named'; index: number }
  | { kind: 'index'; index: number }
  | { kind: 'rgb'; r: number; g: number; b: number }
  | { kind: 'by_value'; field?: string; game?: boolean; steps?: boolean };

/** How a value shows. A bar takes a width of 1 to 80 cells and a color,
 *  `trunc` how many characters it keeps. */
export interface PromptFormatChoice {
  format: PromptFormatName;
  width?: number;
  color?: PromptColorChoice;
  chars?: number;
}

/** The kinds of underline: `underline` is the single line (SGR 4), and
 *  `double`, `curly`, `dotted` and `dashed` are SGR 4:2 to 4:5. One kind
 *  holds at a time. */
export type PromptUnderlineStyle = 'underline' | 'double' | 'curly' | 'dotted' | 'dashed';

/** A style the card turns on or off. An underline kind replaces the kind
 *  a part had, and turning any underline kind off ends the underline. */
export type PromptStyleChoice =
  | 'bold'
  | 'dim'
  | 'italic'
  | PromptUnderlineStyle
  | 'inverse'
  | 'strike'
  | 'blink';

export type PromptWhen = 'always' | 'fight' | 'not_fight';

/** One change to a design. `piece` indexes the template's pieces, as a
 *  span names them. `at` and `to` are places between pieces, from 0
 *  before the first to the number of pieces after the last. */
export type PromptEditOp =
  | { op: 'set_format'; piece: number; format: PromptFormatChoice }
  | {
      op: 'set_color';
      piece: number;
      color: PromptColorChoice;
      background?: boolean;
      underline?: boolean;
    }
  | { op: 'set_style'; piece: number; style: PromptStyleChoice; on: boolean }
  | { op: 'set_when'; piece: number; when: PromptWhen }
  | { op: 'set_text'; piece: number; text: string }
  | { op: 'remove'; piece: number }
  | { op: 'insert_field'; at: number; field: string; format?: PromptFormatChoice }
  | { op: 'insert_text'; at: number; text: string }
  | { op: 'insert_nl'; at: number }
  | { op: 'insert_right'; at: number }
  | { op: 'move'; piece: number; to: number };

/** A design after an edit, drawn with the live values and placeholders. */
export interface PromptEdited {
  template: string;
  rendered: PromptRendered;
  /** Where the piece the edit acted on sits now: the piece it changed or
   *  moved, or the one it added. Null after a removal. */
  piece: number | null;
}

/** Apply one edit to a design. Every other piece keeps its look. Save
 *  the result with promptConfigSet. */
export async function promptEdit(template: string, op: PromptEditOp): Promise<PromptEdited> {
  return invoke('prompt_edit', { template, op });
}

/** What a piece of a design holds, as the card names it. */
export type PromptPieceKind =
  | 'codes'
  | 'text'
  | 'value'
  | 'cur_max'
  | 'percent'
  | 'nl'
  | 'right'
  | 'raw'
  | 'if'
  | 'if_not'
  | 'end'
  | 'unknown';

/** One form a value takes. `segment` is what its Show as segment reads,
 *  the sample when it is short and else the name. */
export interface PromptForm {
  format: PromptFormatName;
  label: string;
  segment: string;
  sample: PromptRendered;
  /** Show as offers it. The picker offers every form. */
  show_as: boolean;
}

/** One piece of a design as the card shows it. `text` is its template
 *  text with its own codes. A max alone reads as its gauge's field in the
 *  form `max`. The colors and styles are its look at its first cell, a
 *  bar's color its cells'. `underline_style` names the kind of underline,
 *  null with none, and `underline_color` its color, `default` for the
 *  text's own. `when_fixed` says a fight condition outside another one
 *  holds it. */
export interface PromptPiece {
  piece: number;
  kind: PromptPieceKind;
  text: string;
  field: string | null;
  label: string;
  format: PromptFormatName | null;
  width: number | null;
  when: PromptWhen;
  when_fixed: boolean;
  color: PromptColorChoice;
  background: PromptColorChoice;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
  underline_style: PromptUnderlineStyle | null;
  underline_color: PromptColorChoice;
  inverse: boolean;
  strike: boolean;
  blink: boolean;
  literal: string | null;

  meta: string | null;
  forms: PromptForm[];
  by_value: boolean;
  shows: boolean;
}

export type PromptTokenKind = 'text' | 'code' | 'value' | 'condition' | 'line' | 'raw' | 'unknown';

/** One token of a design: where it sits in the text in UTF-16 units, the
 *  piece it belongs to, and whether Vosh knows the name it reads. */
export interface PromptToken {
  start: number;
  end: number;
  piece: number;
  kind: PromptTokenKind;
  name: string | null;
  known: boolean;
}

export interface PromptDescribed {
  pieces: PromptPiece[];
  tokens: PromptToken[];
}

/** A Line trigger that matched your prompt as a line (D6). `preset` says
 *  a highlight preset installed it. */
export interface PromptLineTrigger {
  name: string;
  pattern: string;
  preset: boolean;
}

/** The Line triggers that match a prompt `capture` reads in the
 *  candidates ring, which no longer see it once the profile reads it. */
export async function promptLineTriggers(capture: PromptCapture): Promise<PromptLineTrigger[]> {
  return invoke('prompt_line_triggers', { capture });
}

/** What each piece and token of a design is, with what each value reads
 *  in the preview the card shows. */
export async function promptDescribe(
  template: string,
  preview: PromptPreviewName | null = null,
  overrides: PromptOverrides | null = null,
): Promise<PromptDescribed> {
  return invoke('prompt_describe', { template, preview, overrides });
}

/** The forms a field takes, `hp` or `aff:sanctuary`, each drawn as the
 *  card shows it, for the picker. */
export async function promptForms(
  field: string,
  preview: PromptPreviewName | null = null,
): Promise<PromptForm[]> {
  return invoke('prompt_forms', { field, preview });
}

export type PromptFieldGroup =
  | 'vitals'
  | 'fight'
  | 'group'
  | 'character'
  | 'worth'
  | 'affects'
  | 'room'
  | 'time_and_sky'
  | 'vosh'
  | 'scripts'
  | 'building'
  | 'more';

export type PromptFieldKind =
  | 'gauge'
  | 'num'
  | 'pct'
  | 'tank_pct'
  | 'text'
  | 'flag'
  | 'count'
  | 'position'
  | 'lang'
  | 'moon'
  | 'exits'
  | 'level'
  | 'slot'
  | 'hour'
  | 'temp'
  | 'seconds'
  | 'clock'
  | 'date'
  | 'ticks'
  | 'member'
  | 'raw';

/** One field the picker lists, with its state now. */
export interface PromptFieldState {
  name: string;
  label: string;
  aliases: string[];
  kind: PromptFieldKind;
  group: PromptFieldGroup;
  package: string | null;
  /** Only the new server build sends its package. */
  new_build: boolean;
  codes: string[];
  search: string[];
  /** Written with a parameter, `%{aff:sanctuary}`. */
  param: boolean;
  listed: boolean;
  state: 'value' | 'hidden' | 'absent' | 'missing';
  source: 'script' | 'capture' | 'gmcp' | 'vosh' | null;
  /** The value as the picker shows it, an enum as its word. */
  value: string | null;
  max: string | null;
  /** Its package came this session, or it has none. A package older
   *  builds send too counts for a new build field only on the new build. */
  sent: boolean;
  /** Your prompt shows it. */
  in_prompt: boolean;
}

export type PromptStatus = 'no_capture' | 'matching' | 'not_matching' | 'prompts_off';

/** session://prompt-status, and the status in the prompt state.
 *  `last_match_at` is RFC 3339 local time. */
export interface PromptStatusPayload {
  status: PromptStatus;
  last_match_at: string | null;
}

/** The drawn prompt while it is the last thing on screen: its region,
 *  where each piece landed, and the rows it draws as plain text joined by
 *  `\n`, which the webview wraps at its renderer's width to put each
 *  span on screen. */
export interface PromptOpenRow {
  gen: number;
  spans: PromptSpan[];
  plain: string;
  /** The game's own lines the drawn prompt replaced, which the row shows
   *  while the card reads your codes or drawing is off. */
  raw_lines: string[];
  /** The index in the prompt the game sent of the first of them. */
  raw_from: number;
}

/** Everything the card reads about your prompt now. `open_row` is the
 *  drawn prompt while it is the last thing on screen, with where each
 *  piece landed. */
export interface PromptState {
  catalog: PromptFieldState[];
  status: PromptStatusPayload;
  new_build: boolean;
  /** The Forsaken Lands rules hold: the host is The Forsaken Lands or the
   *  capture reads its codes. */
  forsaken: boolean;
  open_row: PromptOpenRow | null;
  /** The GMCP packages that came this session. */
  packages: string[];
}

/** The catalog with live states and sources, the status, the new build
 *  sign and the open row. */
export async function promptStateGet(): Promise<PromptState> {
  return invoke('prompt_state_get');
}

/** While on, session://prompt-state follows each prompt Vosh reads. */
export async function promptWatch(on: boolean): Promise<void> {
  await invoke('prompt_watch', { on });
}

/** The prompt state after each prompt, while the card watches. */
export async function onPromptState(cb: (payload: PromptState) => void): Promise<UnlistenFn> {
  return listen<PromptState>('session://prompt-state', (event) => {
    cb(event.payload);
  });
}

/** Whether Vosh reads your prompt, each time that changes. */
export async function onPromptStatus(
  cb: (payload: PromptStatusPayload) => void,
): Promise<UnlistenFn> {
  return listen<PromptStatusPayload>('session://prompt-status', (event) => {
    cb(event.payload);
  });
}

/** A trigger hid your prompt and set prompt values while this profile
 *  draws nothing in its place, once per trigger per session. */
export async function onPromptGagWithoutReader(
  cb: (payload: { trigger: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ trigger: string }>('session://prompt-gag-without-reader', (event) => {
    cb(event.payload);
  });
}

/** The triggers that hid your prompt this session with nothing drawn in
 *  its place, for a window that opens after the session named them. */
export async function promptGagsWithoutReader(): Promise<string[]> {
  return invoke('prompt_gags_without_reader');
}

// Keyboard macro bindings. A Macro maps a canonical key string
// (produced by canonicalKeyFromEvent below) to a command line that
// the input layer will fire when that key combo is pressed.
export interface Macro {
  key: string;
  command: string;
  /** Optional user-defined group / folder. Like trigger/alias groups,
   *  bulk-disabled via the Settings UI without losing individual
   *  bindings. */
  group?: string | null;
  /** False keeps the binding but lets the key fall through as if it
   *  were not bound. The backend omits the field while it is on, so
   *  absent means on. */
  enabled?: boolean;
}

/** One row in any groups-list response: name + current enabled state.
 *  Backend returns these sorted by name. */
export interface GroupState {
  name: string;
  enabled: boolean;
}

export async function listMacros(): Promise<Macro[]> {
  return invoke('macros_list');
}

/** Bind or rebind a key. `enabled` turns the binding on or off. Leave
 *  it out to keep an existing binding's state, or make a new one on. */
export async function setMacro(
  key: string,
  command: string,
  group: string | null = null,
  enabled?: boolean,
): Promise<Macro[]> {
  return invoke('macros_set', {
    key,
    command,
    group: group && group.length > 0 ? group : null,
    enabled: enabled ?? null,
  });
}

export async function deleteMacro(key: string): Promise<Macro[]> {
  return invoke('macros_delete', { key });
}

/** One interval timer: fire `command` every `interval_secs` seconds while
 *  connected. `id` is a stable backend-assigned handle. A timer in a
 *  group that is off waits as one that is off does. */
export interface Timer {
  id: number;
  name: string;
  interval_secs: number;
  command: string;
  enabled: boolean;
  /** Left out while the timer is in no group. */
  group?: string | null;
}

export async function timersList(): Promise<Timer[]> {
  return invoke('timers_list');
}

/** Create (id null) or update (existing id) a timer. A null group puts
 *  it in none. Returns the full list. */
export async function timersSet(
  id: number | null,
  name: string,
  intervalSecs: number,
  command: string,
  enabled: boolean,
  group: string | null,
): Promise<Timer[]> {
  return invoke('timers_set', { id: id ?? null, name, intervalSecs, command, enabled, group });
}

export async function timersDelete(id: number): Promise<Timer[]> {
  return invoke('timers_delete', { id });
}

// --- Group switches, one on each group heading in Settings, Automation ---

/** A list whose items sit in groups, named as its Automation list is. */
export type GroupList = 'triggers' | 'aliases' | 'macros' | 'timers';

/** What the loadouts decide about a group while they decide it. Every
 *  launch, profile switch and Loadouts save lays it over the group
 *  again, so the switch waits. */
export interface LoadoutHold {
  /** Whether the loadouts turn the group on. */
  on: boolean;
  /** The active loadouts that decide. Empty while every loadout is off. */
  by: string[];
}

/** One group heading's switch. */
export interface GroupSwitch {
  name: string;
  /** Whether the group is on now. */
  enabled: boolean;
  /** Set while the loadouts decide the group. */
  loadouts?: LoadoutHold;
}

/** The switch of each group in one list, sorted by name. */
export async function listGroupSwitches(list: GroupList): Promise<GroupSwitch[]> {
  return invoke('groups_list', { list });
}

/** Turn a whole group of one list on or off. Returns every switch of the
 *  list. Fails for a group the loadouts decide. */
export async function setGroupEnabled(
  list: GroupList,
  group: string,
  enabled: boolean,
): Promise<GroupSwitch[]> {
  return invoke('groups_set_enabled', { list, group, enabled });
}

/** A group of any list turned on or off: #group, Lua, or a switch in
 *  Settings. */
export async function subscribeGroupsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<string>('vosh://groups-changed', () => cb());
}

// --- Macro groups, which the command line follows ---

export async function listMacroGroups(): Promise<GroupState[]> {
  return invoke('macros_groups_list');
}

/** The trigger list changed: Settings saved it, or #trigger, an import,
 *  a preset, or a script edited it. */
export async function subscribeTriggersChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<string>('vosh://triggers-changed', () => cb());
}

/** The alias list changed: Settings saved it, or #alias, an import, or a
 *  Lua mud.alias edited it. */
export async function subscribeAliasesChanged(cb: () => void): Promise<UnlistenFn> {
  return listen<string>('vosh://aliases-changed', () => cb());
}

export async function subscribeMacroGroupsChanged(
  cb: (group: string) => void,
): Promise<UnlistenFn> {
  return listen<string>('vosh://macro-groups-changed', (event) => {
    cb(event.payload);
  });
}

export async function subscribeMacrosChanged(cb: (macros: Macro[]) => void): Promise<UnlistenFn> {
  return listen<Macro[]>('vosh://macros-changed', (event) => {
    cb(event.payload);
  });
}

export async function subscribeTimersChanged(cb: (timers: Timer[]) => void): Promise<UnlistenFn> {
  return listen<Timer[]>('vosh://timers-changed', (event) => {
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

export async function onOutput(cb: (out: SessionOutput) => void): Promise<UnlistenFn> {
  return listen<OutputPayload>('session://output', (event) => {
    cb(decodeOutputPayload(event.payload));
  });
}

/** Write text the webview drew itself, such as your typed echo or an
 *  error notice, into the native grid too, and tell the session, which
 *  closes the open row, since the text now follows it. `after` is the
 *  newest output of the prompt stage xterm took before the text while
 *  xterm shows, and null while the native grid shows, which names its
 *  own. The session can hear of your echo after the reply to your line,
 *  and the prompt that came after the echo stays open. */
export async function terminalLocalWrite(text: string, after: number | null): Promise<void> {
  await invoke('terminal_local_write', { text, after });
}

/** You started or stopped selecting text or reading back in xterm. While
 *  you do, a clock piece in your design leaves your prompt in the text as
 *  it is. */
export async function terminalReaderBusy(busy: boolean): Promise<void> {
  await invoke('terminal_reader_busy', { busy });
}

/** Where the native grid's cursor sits and where its open region
 *  starts. Lines count from the top of the live screen, negative in
 *  history, so while `at_bottom` holds a line is the screen row the grid
 *  draws it on. `region` is null once anything lands after the region. */
export interface TerminalCursor {
  line: number;
  col: number;
  at_bottom: boolean;
  cols: number;
  region: { gen: number; line: number; col: number } | null;
}

/** The native grid's cursor and open region, for mapping a pointer to a
 *  piece of your prompt while the native renderer draws the terminal.
 *  Null before the grid exists and on a build without it. xterm reads its
 *  own buffer instead. */
export async function terminalCursor(): Promise<TerminalCursor | null> {
  return invoke('terminal_cursor');
}

/** The native grid's live screen as text: each row with trailing blanks
 *  gone, its width, and whether it shows the live tail. */
export interface TerminalScreenRows {
  rows: string[];
  cols: number;
  at_bottom: boolean;
}

/** The native grid's live screen as text, for the prompt card's marks
 *  while the profile reads no prompt. Null before the grid exists and on
 *  a build without it. xterm reads its own buffer instead. */
export async function terminalScreenRows(): Promise<TerminalScreenRows | null> {
  return invoke('terminal_screen_rows');
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

export interface ScrollbackLoad {
  bytes: Uint8Array;
  /** True when this load also seeded the native grid with the bytes. Only
   *  the first seed request per process does, so a reloaded page sees
   *  false while the grid still holds the history. */
  seededNative: boolean;
}

export async function loadScrollback(feedNative = false): Promise<ScrollbackLoad> {
  const res = await invoke<{ bytes: number[]; seeded_native: boolean }>('scrollback_load', {
    feedNative,
  });
  return { bytes: new Uint8Array(res.bytes), seededNative: res.seeded_native };
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

/** Caret shapes the command line can paint. Each one renders inside the
 *  same anchor box as the default block, so switching shapes never
 *  reflows the input row. */
export const INPUT_CURSOR_STYLES = [
  'block',
  'block_outline',
  'half_block',
  'underline',
  'underline_thick',
  'pipe',
  'pipe_thick',
] as const;

export type InputCursorStyle = (typeof INPUT_CURSOR_STYLES)[number];

/** Coerce an unknown caret shape back to the default block. */
export function normalizeInputCursorStyle(value: unknown): InputCursorStyle {
  return INPUT_CURSOR_STYLES.includes(value as InputCursorStyle)
    ? (value as InputCursorStyle)
    : 'block';
}

/** Terminal row spacing. Each id maps to the multiple of the glyph
 *  height that xterm takes as `lineHeight`, and the native grid follows
 *  through the cell size xterm reports. */
export const TERMINAL_LINE_HEIGHTS = {
  compact: 1.1,
  default: 1.2,
  loose: 1.35,
} as const;

export type TerminalLineHeight = keyof typeof TERMINAL_LINE_HEIGHTS;

/** Coerce an unknown line height id back to the default. */
export function normalizeTerminalLineHeight(value: unknown): TerminalLineHeight {
  return value === 'compact' || value === 'loose' ? value : 'default';
}

/** How the vitals under the panel's panes lay out. `rows` gives each
 *  vital its own row. `line` sets Health, Mana, and Moves side by side
 *  on one row. */
export const VITALS_DENSITIES = ['rows', 'line'] as const;

export type VitalsDensity = (typeof VITALS_DENSITIES)[number];

/** Coerce an unknown vitals density back to rows. */
export function normalizeVitalsDensity(value: unknown): VitalsDensity {
  return value === 'line' ? 'line' : 'rows';
}

/** What each vital's value shows. `current-max` reads `186 / 1020`,
 *  `current` reads `186`, and `percent` reads `18%`. */
export const VITALS_VALUES = ['current-max', 'current', 'percent'] as const;

export type VitalsValues = (typeof VITALS_VALUES)[number];

/** Coerce an unknown value form back to current and max. */
export function normalizeVitalsValues(value: unknown): VitalsValues {
  return value === 'current' || value === 'percent' ? value : 'current-max';
}

/** The meter under each vital. `line` is the 2 px meter, `bar` the
 *  4 px one, and `none` drops the meters and tightens the rows. */
export const VITALS_METERS = ['line', 'bar', 'none'] as const;

export type VitalsMeter = (typeof VITALS_METERS)[number];

/** Coerce an unknown meter back to the line. */
export function normalizeVitalsMeter(value: unknown): VitalsMeter {
  return value === 'bar' || value === 'none' ? value : 'line';
}

/** The vitals rows that join Density under Layout, Vitals, as one
 *  event payload. The panel footer reads the first three, and the panel
 *  reads the last to drop the footer. The status line reads the values
 *  and the warning, never the meter. */
export interface VitalsOptions {
  values: VitalsValues;
  meter: VitalsMeter;
  /** Warn under two thirds and turn danger under one third, like the
   *  Group pane. Off keeps danger under 20 percent. */
  warn_thirds: boolean;
  /** Hide the panel's vitals while your prompt is pinned. */
  hide_when_pinned: boolean;
}

export const DEFAULT_VITALS_OPTIONS: VitalsOptions = {
  values: 'current-max',
  meter: 'line',
  warn_thirds: false,
  hide_when_pinned: true,
};

/** The vitals options a config holds. */
export function vitalsOptionsOf(
  config: Pick<
    UiConfig,
    'vitals_values' | 'vitals_meter' | 'vitals_warn_thirds' | 'vitals_hide_when_pinned'
  >,
): VitalsOptions {
  return {
    values: config.vitals_values,
    meter: config.vitals_meter,
    warn_thirds: config.vitals_warn_thirds,
    hide_when_pinned: config.vitals_hide_when_pinned,
  };
}

/** Read vitals options off the bus, filling anything missing or
 *  unknown with the defaults. */
export function normalizeVitalsOptions(raw: unknown): VitalsOptions {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    values: normalizeVitalsValues(o.values),
    meter: normalizeVitalsMeter(o.meter),
    warn_thirds: o.warn_thirds === true,
    hide_when_pinned: o.hide_when_pinned !== false,
  };
}

/** The layouts the Affects pane draws. `timers` is Timers first, the
 *  default, with your tracked affects in their slots. `countdown` lists
 *  every affect by the hours it has left. `chips` groups them as chips,
 *  what to recast first. `chips_drain` groups them the same way, and a
 *  chip running out colors only the share of it that matches the hours
 *  it has left. */
export const AFFECTS_STYLES = ['timers', 'countdown', 'chips', 'chips_drain'] as const;

export type AffectsStyle = (typeof AFFECTS_STYLES)[number];

/** Grouped chips and Draining chips, the styles that pack chips and
 *  show the state on each, with no marker or tint of their own. */
export function isChipsStyle(style: AffectsStyle): boolean {
  return style === 'chips' || style === 'chips_drain';
}

/** Coerce an unknown affects layout back to Timers first. */
export function normalizeAffectsStyle(value: unknown): AffectsStyle {
  return AFFECTS_STYLES.find((style) => style === value) ?? 'timers';
}

/** The mark beside each tracked affect in the timers and countdown
 *  layouts. `dot` is the default. `none` draws no mark. */
export const AFFECTS_MARKERS = ['dot', 'square', 'plus_minus', 'none'] as const;

export type AffectsMarker = (typeof AFFECTS_MARKERS)[number];

/** Coerce an unknown affects marker back to the dot. */
export function normalizeAffectsMarker(value: unknown): AffectsMarker {
  return AFFECTS_MARKERS.find((marker) => marker === value) ?? 'dot';
}

/** The most hours either affects threshold takes. */
export const AFFECTS_HOURS_MAX = 99;

/** Whole hours from 0 to AFFECTS_HOURS_MAX, or null when `value` is no
 *  number. */
function affectsHoursOf(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null;
  return Math.min(AFFECTS_HOURS_MAX, Math.max(0, Math.round(value)));
}

/** The hours at which an affect runs out and is almost gone, read the
 *  way the backend saves them: whole hours from 0 to 99, almost gone
 *  never over running out, and the defaults, 2 and 1, for anything
 *  that is no number. */
export function normalizeAffectsThresholds(
  runningOut: unknown,
  almostGone: unknown,
): { running_out: number; almost_gone: number } {
  const running_out = affectsHoursOf(runningOut) ?? EXPIRING_TICKS;
  const almost_gone = Math.min(affectsHoursOf(almostGone) ?? CRITICAL_TICKS, running_out);
  return { running_out, almost_gone };
}

/** How the Affects pane draws, as one event payload. Style and Marker
 *  from Settings, Layout, Affects or the pane's own menu, and Tint what
 *  to recast and the two thresholds from Settings. */
export interface AffectsDisplay {
  style: AffectsStyle;
  marker: AffectsMarker;
  /** Tint the missing and running out rows in the timers and countdown
   *  layouts. Grouped chips always do. */
  tint: boolean;
  /** At or under this many hours an affect you track turns yellow and
   *  counts as running out. `affects_running_out_hours`. */
  running_out: number;
  /** At or under this many hours an affect's hours turn bold red.
   *  `affects_almost_gone_hours`. */
  almost_gone: number;
}

export const DEFAULT_AFFECTS_DISPLAY: AffectsDisplay = {
  style: 'timers',
  marker: 'dot',
  tint: false,
  running_out: EXPIRING_TICKS,
  almost_gone: CRITICAL_TICKS,
};

/** The fields of a config that hold the affects display. */
export type AffectsDisplayFields = Pick<
  UiConfig,
  | 'affects_style'
  | 'affects_marker'
  | 'affects_tint'
  | 'affects_running_out_hours'
  | 'affects_almost_gone_hours'
>;

/** The affects display a config holds. */
export function affectsDisplayOf(config: AffectsDisplayFields): AffectsDisplay {
  return {
    style: config.affects_style,
    marker: config.affects_marker,
    tint: config.affects_tint,
    running_out: config.affects_running_out_hours,
    almost_gone: config.affects_almost_gone_hours,
  };
}

/** The config fields that hold `display`, to lay over a config copy. */
export function affectsDisplayFields(display: AffectsDisplay): AffectsDisplayFields {
  return {
    affects_style: display.style,
    affects_marker: display.marker,
    affects_tint: display.tint,
    affects_running_out_hours: display.running_out,
    affects_almost_gone_hours: display.almost_gone,
  };
}

/** Whether two affects displays draw the pane alike. */
export function sameAffectsDisplay(a: AffectsDisplay, b: AffectsDisplay): boolean {
  return (
    a.style === b.style &&
    a.marker === b.marker &&
    a.tint === b.tint &&
    a.running_out === b.running_out &&
    a.almost_gone === b.almost_gone
  );
}

/** Read an affects display off the bus, filling anything missing or
 *  unknown with the defaults. */
export function normalizeAffectsDisplay(raw: unknown): AffectsDisplay {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    style: normalizeAffectsStyle(o.style),
    marker: normalizeAffectsMarker(o.marker),
    tint: o.tint === true,
    ...normalizeAffectsThresholds(o.running_out, o.almost_gone),
  };
}

/** The light theme a profile starts with. */
export const DEFAULT_LIGHT_THEME_ID = 'vellum';

/** The dark theme a profile that never saved one starts with: its
 *  current theme when that theme is dark, else Obsidian Ember. */
export function seedDarkTheme(theme: string, customThemes: CustomTheme[]): string {
  const custom = customThemes.find((t) => t.id === theme);
  const found = custom ? customToAppTheme(custom) : BUILTIN_THEMES.find((t) => t.id === theme);
  return found && themeTokens(found).appearance === 'dark' ? theme : DEFAULT_THEME_ID;
}

/** Move every custom theme whose id a built-in theme now has to a free
 *  id of its own, and point the theme choices that named it there. A
 *  theme you imported before Vosh shipped one under the same id (a
 *  Solarized Light file reads as solarized-light) would otherwise hide
 *  behind the built-in. findTheme returns the built-in, the gallery
 *  shows two tiles under one id, and your edits never reach the screen.
 *  Until a built-in took the id, a choice that named it meant the custom
 *  theme, the first one when two shared it. Returns `cfg` itself when no
 *  id collides. */
export function freeBuiltinThemeIds(cfg: RawUiConfig): RawUiConfig {
  const customs = Array.isArray(cfg.custom_themes) ? cfg.custom_themes : [];
  const builtinIds = new Set(BUILTIN_THEMES.map((t) => t.id));
  if (!customs.some((t) => builtinIds.has(t.id))) return cfg;
  const taken = new Set([...builtinIds, ...customs.map((t) => t.id)]);
  const moved = new Map<string, string>();
  const custom_themes = customs.map((t) => {
    if (!builtinIds.has(t.id)) return t;
    const id = uniqueThemeId(t.id, taken);
    taken.add(id);
    if (!moved.has(t.id)) moved.set(t.id, id);
    return { ...t, id };
  });
  const out: RawUiConfig = { ...cfg, custom_themes };
  for (const key of ['theme', 'light_theme', 'dark_theme'] as const) {
    const id = cfg[key];
    const to = typeof id === 'string' ? moved.get(id) : undefined;
    if (to !== undefined) out[key] = to;
  }
  return out;
}

/** Where your prompt shows: in the text, lifted on a band in the text,
 *  or pinned on a band above the command line. */
export type PromptShow = 'text' | 'lifted' | 'pinned';

export const PROMPT_SHOWS: readonly PromptShow[] = ['text', 'lifted', 'pinned'];

/** A known place, or the text for anything else, so a value a newer
 *  build wrote never breaks this one. */
export function normalizePromptShow(value: unknown): PromptShow {
  return typeof value === 'string' && (PROMPT_SHOWS as readonly string[]).includes(value)
    ? (value as PromptShow)
    : 'text';
}

export interface UiConfig {
  /** The theme you picked. Vosh shows it while follow_system_appearance
   *  is off. */
  theme: ThemeChoice;
  /** Show light_theme or dark_theme to match the OS appearance. */
  follow_system_appearance: boolean;
  /** The theme shown while following the system and the OS is light. */
  light_theme: string;
  /** The theme shown while following the system and the OS is dark. */
  dark_theme: string;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  /** Terminal row spacing, one of TERMINAL_LINE_HEIGHTS. */
  terminal_line_height: TerminalLineHeight;
  tracked_affects: TrackedAffect[];
  enabled_presets: string[];
  keep_last_command: boolean;
  /** Tri-state: true / false are explicit user choices; null means
   *  the default, which is on. Resolve with resolveThemeTerminalColors
   *  before use. */
  theme_terminal_colors: boolean | null;
  bright_bold: boolean;
  /** Blinking text. Tri-state: true and false are your choice, null
   *  means none, which reads as on unless your system reduces motion.
   *  Resolve with resolveBlinkText before use. */
  blink_text: boolean | null;
  /** Keep highlight colors readable. While on, the session draws a true
   *  color a trigger paints text in at a lightness that reads on the
   *  theme's terminal background. On unless you turn it off. */
  readable_highlights: boolean;
  /** Collapse repeated lines. While on, the session shows a line the
   *  game sends that reads exactly as the line before it on screen,
   *  colors included, once with a count before it. Off unless you turn
   *  it on. */
  collapse_repeats: boolean;
  /** Custom base terminal palette: 16 CSS colors in ANSI 0-15 order,
   *  used whenever the tint toggle resolves off. Null = canonical
   *  xterm chart. */
  terminal_base_ansi: string[] | null;
  custom_themes: CustomTheme[];
  /** Override color for the split-scrollback divider. Empty/undefined
   *  means use the theme default (--c-border). */
  split_divider_color: string | null;
  /** Override color for locally-echoed sent input. Empty/undefined
   *  means no recoloring (default terminal foreground). */
  input_echo_color: string | null;
  /** When true (default), commands sent by keyboard macros echo
   *  locally like typed commands, so under lag the keybind visibly
   *  registered before the world responds. */
  echo_macros: boolean;
  /** When true (default), each command you send echoes after a grey
   *  `›` and a space, Mark your commands under Input in Settings. */
  input_echo_caret: boolean;
  /** Milliseconds to wait between lines when sending a multi-line
   *  paste. 0 = no pacing; non-zero spreads sends out so the MUD
   *  flood filter does not kick. Clamped server-side to [0, 10000]. */
  paste_line_delay_ms: number;
  /** When true, the prompt input enables the webview's native spell
   *  check, but only when the current line starts with a chat verb
   *  (say / tell / chat / gossip / ooc / clan / immtalk / reply /
   *  ' / "). Plain commands stay un-checked so MUD verbs like
   *  `kill` / `oload` / alias names do not light up red. Default
   *  off; opt-in for roleplayers. */
  spellcheck_prompt: boolean;
  /** Shape of the command-line caret. Defaults to the ember block. */
  input_cursor_style: InputCursorStyle;
  /** How the vitals under the panel's panes lay out, one of
   *  VITALS_DENSITIES. */
  vitals_density: VitalsDensity;
  /** What each vital's value shows, one of VITALS_VALUES. */
  vitals_values: VitalsValues;
  /** The meter under each vital, one of VITALS_METERS. */
  vitals_meter: VitalsMeter;
  /** Warn before you run low, by the Group pane's thirds. */
  vitals_warn_thirds: boolean;
  /** Hide the panel's vitals while your prompt is pinned, so the panes
   *  take their room. On unless you turn it off. */
  vitals_hide_when_pinned: boolean;
  /** How the status line draws the tick, the game time, and the moons.
   *  The value alone, a caption before each value, or an icon before
   *  each. The moons are icons already, so only Caption changes them. */
  chip_style: ChipStyle;
  /** Which way the status line tick counts, one of TICK_COUNTS. */
  tick_count: TickCount;
  /** The clock the status line reads the game time on, one of
   *  GAME_TIMES. */
  game_time: GameTime;
  /** The Affects pane's layout, one of AFFECTS_STYLES. */
  affects_style: AffectsStyle;
  /** The mark beside each tracked affect, one of AFFECTS_MARKERS. */
  affects_marker: AffectsMarker;
  /** Tint what to recast in the timers and countdown layouts. */
  affects_tint: boolean;
  /** At or under this many hours an affect you track turns yellow and
   *  counts as running out. Whole hours from 0 to 99. */
  affects_running_out_hours: number;
  /** At or under this many hours an affect's hours turn bold red. Whole
   *  hours from 0 to 99, never over affects_running_out_hours. */
  affects_almost_gone_hours: number;
  /** How many times the backend had replaced the live config when this
   *  copy was read. setUiConfig sends it back, and the backend turns
   *  away a save from a copy read before a later replace. Absent on a
   *  config that never came from the backend. */
  generation?: number;
}

export type ChipStyle = 'value_only' | 'caption_value' | 'icon_value';

/** Read a stored or broadcast chip style. Anything unknown is the
 *  value alone, the default. */
export function normalizeChipStyle(raw: unknown): ChipStyle {
  return raw === 'caption_value' || raw === 'icon_value' ? raw : 'value_only';
}

/** The ways the status line tick counts. `up`, the default, shows the
 *  seconds since the last tick. `down` shows the seconds left until the
 *  next and waits at 0 while the game runs late. `down_past_zero`
 *  counts on below zero until the tick lands. */
export const TICK_COUNTS = ['up', 'down', 'down_past_zero'] as const;
export type TickCount = (typeof TICK_COUNTS)[number];

/** Read a stored or broadcast tick count. Anything unknown counts up. */
export function normalizeTickCount(raw: unknown): TickCount {
  return TICK_COUNTS.find((count) => count === raw) ?? 'up';
}

/** The clocks the status line reads the game time on. `24h`, the
 *  default, reads like 18:00, and `12h` like 6:00 PM. */
export const GAME_TIMES = ['24h', '12h'] as const;
export type GameTime = (typeof GAME_TIMES)[number];

/** Read a stored or broadcast clock. Anything unknown is the 24 hour
 *  clock. */
export function normalizeGameTime(raw: unknown): GameTime {
  return GAME_TIMES.find((clock) => clock === raw) ?? '24h';
}

// Dedupe the mount-time burst: App, Input, and the tracked affects
// store all call getUiConfig on first render. Sharing one
// in-flight promise turns that into a single IPC round-trip. The cache
// clears once resolved, so a later call (after a config change) still
// re-fetches fresh — no staleness.
let uiConfigInFlight: Promise<UiConfig> | null = null;
export function getUiConfig(): Promise<UiConfig> {
  if (!uiConfigInFlight) {
    uiConfigInFlight = fetchUiConfig().finally(() => {
      uiConfigInFlight = null;
    });
  }
  return uiConfigInFlight;
}

/** The raw shape `ui_get_config` returns, before normalizeUiConfig
 *  fills gaps and coerces unknown values. */
export interface RawUiConfig {
  theme: string;
  follow_system_appearance?: boolean;
  light_theme?: string;
  dark_theme?: string;
  auto_update: boolean;
  font_family: string;
  font_size: number;
  terminal_line_height?: string;
  tracked_affects: unknown[];
  enabled_presets: string[];
  keep_last_command?: boolean;
  theme_terminal_colors?: boolean;
  bright_bold?: boolean;
  blink_text?: boolean | null;
  readable_highlights?: boolean;
  collapse_repeats?: boolean;
  terminal_base_ansi?: unknown;
  custom_themes?: CustomTheme[];
  split_divider_color?: string | null;
  input_echo_color?: string | null;
  echo_macros?: boolean;
  input_echo_caret?: boolean;
  paste_line_delay_ms?: number;
  spellcheck_prompt?: boolean;
  input_cursor_style?: string;
  vitals_density?: string;
  vitals_values?: string;
  vitals_meter?: string;
  vitals_warn_thirds?: boolean;
  vitals_hide_when_pinned?: boolean;
  chip_style?: string;
  tick_count?: string;
  game_time?: string;
  affects_style?: string;
  affects_marker?: string;
  affects_tint?: boolean;
  affects_running_out_hours?: number;
  affects_almost_gone_hours?: number;
  generation?: number;
}

async function fetchUiConfig(): Promise<UiConfig> {
  const raw = await invoke<RawUiConfig>('ui_get_config');
  const freed = freeBuiltinThemeIds(raw);
  const config = normalizeUiConfig(freed);
  // A custom theme moved off a built-in id is saved under its new id at
  // once. Later reads then find no collision, so a choice that names
  // the built-in keeps meaning the built-in. Every window moves it the
  // same way, so the save sends no events. A save the backend turns
  // away met a replace, and that replace reads the config again.
  if (freed !== raw) {
    try {
      await invoke('ui_set_config', { config: uiConfigPayload(config) });
    } catch (e) {
      console.error('[themes] saving the moved custom themes failed', e);
    }
  }
  return config;
}

/** Fill gaps and coerce unknown values in a raw config so every window
 *  reads the same shape. A gap takes the value Rust sends for a profile
 *  that sets nothing, and fixtures/ui-config/defaults.json holds both
 *  sides to those values. Custom themes leave the ids built-in themes
 *  have (freeBuiltinThemeIds). */
export function normalizeUiConfig(raw: RawUiConfig): UiConfig {
  const cfg = freeBuiltinThemeIds(raw);
  const theme =
    typeof cfg.theme === 'string' && cfg.theme.length > 0 ? cfg.theme : DEFAULT_THEME_ID;
  const customThemes = Array.isArray(cfg.custom_themes) ? cfg.custom_themes : [];
  const thresholds = normalizeAffectsThresholds(
    cfg.affects_running_out_hours,
    cfg.affects_almost_gone_hours,
  );
  return {
    theme,
    follow_system_appearance: cfg.follow_system_appearance === true,
    light_theme:
      typeof cfg.light_theme === 'string' && cfg.light_theme.length > 0
        ? cfg.light_theme
        : DEFAULT_LIGHT_THEME_ID,
    dark_theme:
      typeof cfg.dark_theme === 'string' && cfg.dark_theme.length > 0
        ? cfg.dark_theme
        : seedDarkTheme(theme, customThemes),
    auto_update: cfg.auto_update,
    font_family: cfg.font_family,
    font_size: cfg.font_size,
    terminal_line_height: normalizeTerminalLineHeight(cfg.terminal_line_height),
    tracked_affects: Array.isArray(cfg.tracked_affects)
      ? normalizeTrackedAffects(cfg.tracked_affects)
      : [],
    enabled_presets: Array.isArray(cfg.enabled_presets) ? cfg.enabled_presets : [],
    keep_last_command: Boolean(cfg.keep_last_command),
    theme_terminal_colors:
      typeof cfg.theme_terminal_colors === 'boolean' ? cfg.theme_terminal_colors : null,
    bright_bold: Boolean(cfg.bright_bold),
    blink_text: typeof cfg.blink_text === 'boolean' ? cfg.blink_text : null,
    readable_highlights: cfg.readable_highlights !== false,
    collapse_repeats: cfg.collapse_repeats === true,
    terminal_base_ansi:
      Array.isArray(cfg.terminal_base_ansi) &&
      cfg.terminal_base_ansi.length === 16 &&
      cfg.terminal_base_ansi.every((c) => typeof c === 'string' && c.length > 0)
        ? (cfg.terminal_base_ansi as string[])
        : null,
    custom_themes: customThemes,
    split_divider_color:
      typeof cfg.split_divider_color === 'string' && cfg.split_divider_color.length > 0
        ? cfg.split_divider_color
        : null,
    input_echo_color:
      typeof cfg.input_echo_color === 'string' && cfg.input_echo_color.length > 0
        ? cfg.input_echo_color
        : null,
    echo_macros: cfg.echo_macros !== false,
    input_echo_caret: cfg.input_echo_caret !== false,
    paste_line_delay_ms:
      typeof cfg.paste_line_delay_ms === 'number' && cfg.paste_line_delay_ms >= 0
        ? Math.min(10_000, Math.floor(cfg.paste_line_delay_ms))
        : 500,
    spellcheck_prompt: Boolean(cfg.spellcheck_prompt),
    input_cursor_style: normalizeInputCursorStyle(cfg.input_cursor_style),
    vitals_density: normalizeVitalsDensity(cfg.vitals_density),
    vitals_values: normalizeVitalsValues(cfg.vitals_values),
    vitals_meter: normalizeVitalsMeter(cfg.vitals_meter),
    vitals_warn_thirds: cfg.vitals_warn_thirds === true,
    vitals_hide_when_pinned: cfg.vitals_hide_when_pinned !== false,
    chip_style: normalizeChipStyle(cfg.chip_style),
    tick_count: normalizeTickCount(cfg.tick_count),
    game_time: normalizeGameTime(cfg.game_time),
    affects_style: normalizeAffectsStyle(cfg.affects_style),
    affects_marker: normalizeAffectsMarker(cfg.affects_marker),
    affects_tint: cfg.affects_tint === true,
    affects_running_out_hours: thresholds.running_out,
    affects_almost_gone_hours: thresholds.almost_gone,
    ...(typeof cfg.generation === 'number' ? { generation: cfg.generation } : {}),
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

const TERMINAL_LINE_HEIGHT_EVENT = 'vosh://terminal-line-height-changed';
const BLINK_TEXT_EVENT = 'vosh://blink-text-changed';
const VITALS_DENSITY_EVENT = 'vosh://vitals-density-changed';
const VITALS_OPTIONS_EVENT = 'vosh://vitals-options-changed';
const AFFECTS_DISPLAY_EVENT = 'vosh://affects-display-changed';
const READABLE_HIGHLIGHTS_EVENT = 'vosh://readable-highlights-changed';

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

// Cheap deep-equality for the structured fields. Each one, like
// custom_themes, is a small bounded object, so JSON round-trip is
// faster (and more predictable) than a hand-rolled walker.
function deepEqual<T>(a: T, b: T): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

// Fire the cross-window `vosh://*-changed` event fan-out for every
// field in `config` that differs from the last-broadcast snapshot.
// `setUiConfig` calls this after writing to disk, and the main window's
// followReplacedUiConfig sends every field through it after a replace,
// so every window's per-field subscriber sees the updated value.
export async function broadcastUiConfigChanges(config: UiConfig): Promise<void> {
  const prev = lastSentConfig;
  lastSentConfig = config;
  await emitChanged(
    'vosh://custom-themes-changed',
    config.custom_themes,
    prev?.custom_themes,
    deepEqual,
  );
  // The four theme fields go out whole so Settings and the palette keep
  // current copies. theme-changed carries the id they resolve to, which
  // is `theme` unless follow is on. Both come back to this window too,
  // so note them first as its own.
  const prefs = themePrefsOf(config);
  const prevPrefs = prev ? themePrefsOf(prev) : undefined;
  if (!prevPrefs || !deepEqual(prefs, prevPrefs)) noteThemeEcho(prefs);
  await emitChanged(THEME_PREFS_EVENT, prefs, prevPrefs, deepEqual);
  const systemDark = systemPrefersDark();
  const shown = resolveActiveTheme(config, systemDark);
  const prevShown = prev ? resolveActiveTheme(prev, systemDark) : undefined;
  if (shown !== prevShown) noteThemeEcho(shown);
  await emitChanged('vosh://theme-changed', shown, prevShown);
  await emitChanged(
    'vosh://font-changed',
    { family: config.font_family, size: config.font_size },
    prev ? { family: prev.font_family, size: prev.font_size } : undefined,
    (a, b) => a.family === b.family && a.size === b.size,
  );
  await emitChanged(
    TERMINAL_LINE_HEIGHT_EVENT,
    config.terminal_line_height,
    prev?.terminal_line_height,
  );
  await emitChanged('vosh://keep-last-changed', config.keep_last_command, prev?.keep_last_command);
  // The event carries the RESOLVED boolean so listeners never see the
  // tri-state. Resolving both sides of the diff means a theme switch
  // with the setting on auto also fires this event when the effective
  // value flips.
  await emitChanged(
    'vosh://theme-terminal-colors-changed',
    resolveThemeTerminalColors(config.theme, config.theme_terminal_colors),
    prev ? resolveThemeTerminalColors(prev.theme, prev.theme_terminal_colors) : undefined,
  );
  await emitChanged('vosh://bright-bold-changed', config.bright_bold, prev?.bright_bold);
  // Your choice as you made it. Each window reads its own system's
  // reduce motion setting to resolve none.
  await emitChanged(BLINK_TEXT_EVENT, config.blink_text, prev?.blink_text);
  await emitChanged(
    READABLE_HIGHLIGHTS_EVENT,
    config.readable_highlights,
    prev?.readable_highlights,
  );
  await emitChanged(
    'vosh://base-ansi-changed',
    config.terminal_base_ansi,
    prev?.terminal_base_ansi,
    deepEqual,
  );
  await emitChanged(
    'vosh://split-divider-changed',
    config.split_divider_color,
    prev?.split_divider_color,
  );
  await emitChanged(
    'vosh://input-echo-color-changed',
    config.input_echo_color,
    prev?.input_echo_color,
  );
  await emitChanged('vosh://echo-macros-changed', config.echo_macros, prev?.echo_macros);
  await emitChanged(
    'vosh://input-echo-caret-changed',
    config.input_echo_caret,
    prev?.input_echo_caret,
  );
  await emitChanged(
    'vosh://paste-line-delay-changed',
    config.paste_line_delay_ms,
    prev?.paste_line_delay_ms,
  );
  await emitChanged(
    'vosh://spellcheck-prompt-changed',
    config.spellcheck_prompt,
    prev?.spellcheck_prompt,
  );
  await emitChanged(
    'vosh://input-cursor-style-changed',
    config.input_cursor_style,
    prev?.input_cursor_style,
  );
  await emitChanged(VITALS_DENSITY_EVENT, config.vitals_density, prev?.vitals_density);
  await emitChanged(
    VITALS_OPTIONS_EVENT,
    vitalsOptionsOf(config),
    prev ? vitalsOptionsOf(prev) : undefined,
    deepEqual,
  );
  await emitChanged('vosh://chip-style-changed', config.chip_style, prev?.chip_style);
  await emitChanged('vosh://tick-count-changed', config.tick_count, prev?.tick_count);
  await emitChanged('vosh://game-time-changed', config.game_time, prev?.game_time);
  const display = affectsDisplayOf(config);
  const prevDisplay = prev ? affectsDisplayOf(prev) : undefined;
  if (!prevDisplay || !deepEqual(display, prevDisplay)) noteAffectsDisplayEcho(display);
  await emitChanged(AFFECTS_DISPLAY_EVENT, display, prevDisplay, deepEqual);
  await emitChanged(
    TRACKED_AFFECTS_EVENT,
    config.tracked_affects,
    prev?.tracked_affects,
    deepEqual,
  );
}

// Adopt `config` as this window's last-broadcast snapshot without
// emitting anything. A window that reads its config again after a
// replace calls this, since the main window sends every window the new
// values, so its next save diffs against the new profile rather than
// the old.
export function primeUiConfigBroadcast(config: UiConfig): void {
  lastSentConfig = config;
}

/** The backend replaced the live profile's whole UI config, on a
 *  profile switch, a #profile load or reset, or an import. It comes
 *  after the events that carry the panes, the tracked affects, the tick
 *  settings, and the chip style. */
export const UI_CONFIG_REPLACED_EVENT = 'vosh://ui-config-replaced';

/** Hear that the backend replaced the whole UI config. */
export async function subscribeUiConfigReplaced(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>(UI_CONFIG_REPLACED_EVENT, () => cb());
}

/** The reads followReplacedUiConfig runs in this window. A save the
 *  backend turned away runs them too. */
const replaceFollowers = new Set<() => void>();

/** How followReplacedUiConfig hands a window the replaced config. */
export interface FollowReplacedOptions {
  /** After `apply`, send every field to every window. The main window
   *  does this, since Input, the vitals, the prompt, and the other
   *  per-field listeners follow those events, and the backend sends
   *  only a few of them. A diff against this window's last broadcast
   *  could skip a field, since saves from Settings never move it. */
  broadcast?: boolean;
}

/** Keep a window on the live profile's UI config. The backend replaces
 *  it on a profile switch, a #profile load or reset, or an import.
 *  Every replace reads the config again, never sharing a read that
 *  started before it, and hands it to `apply`. Only the newest read
 *  applies.
 *
 *  A window that saves the whole UiConfig (Settings) sends every field
 *  with each save, so a copy from before the replace would write the
 *  old profile's values back. The backend turns such a save away, and
 *  that reads the config again here too. The read becomes this
 *  window's last broadcast, so the next save sends only what you
 *  change, since the main window has sent the rest. The main window
 *  passes `broadcast` and sends them. */
export async function followReplacedUiConfig(
  apply: (config: UiConfig) => void,
  onError: (error: unknown) => void,
  options: FollowReplacedOptions = {},
): Promise<UnlistenFn> {
  let latestRead = 0;
  const reread = () => {
    const mine = ++latestRead;
    fetchUiConfig()
      .then(async (config) => {
        if (mine !== latestRead) return;
        if (!options.broadcast) {
          primeUiConfigBroadcast(config);
          apply(config);
          return;
        }
        apply(config);
        lastSentConfig = null;
        await broadcastUiConfigChanges(config);
      })
      .catch(onError);
  };
  replaceFollowers.add(reread);
  const unlisten = await subscribeUiConfigReplaced(reread);
  return () => {
    replaceFollowers.delete(reread);
    unlisten();
  };
}

/** Sent to every window when the active profile's `[prompt]` table
 *  changed, by `#prompt`, `#unprompt`, the card or a Settings save. */
export const PROMPT_CONFIG_CHANGED_EVENT = 'vosh://prompt-config-changed';

/** The payload of vosh://prompt-config-changed: the active profile
 *  whose table changed, null before any profile loads. */
export interface PromptConfigChangedPayload {
  profile: string | null;
}

/** Hear that the active profile's `[prompt]` table changed. */
export async function subscribePromptConfigChanged(
  cb: (payload: PromptConfigChangedPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PROMPT_CONFIG_CHANGED_EVENT, (event) => {
    const raw = event.payload as { profile?: unknown } | null;
    cb({ profile: typeof raw?.profile === 'string' ? raw.profile : null });
  });
}

// Adopt a theme another window already applied and broadcast, and
// nothing else, so this window's next save does not emit it again
// while its own unsaved edits still diff as changes.
export function primeUiConfigTheme(theme: string): void {
  if (lastSentConfig) lastSentConfig = { ...lastSentConfig, theme };
}

// Adopt the four theme fields another window saved (the palette's
// Choose theme), the same way.
export function primeUiConfigThemePrefs(prefs: ThemePrefs): void {
  if (lastSentConfig) lastSentConfig = { ...lastSentConfig, ...themePrefsOf(prefs) };
}

// Adopt an affects display the pane menu saved, the same way, so this
// window's next save does not send it on as its own change.
export function primeUiConfigAffectsDisplay(display: AffectsDisplay): void {
  if (!lastSentConfig) return;
  lastSentConfig = { ...lastSentConfig, ...affectsDisplayFields(display) };
}

// Every window hears its own affects display broadcast too. One this
// window sent in the last second is its own echo, and adopting it could
// undo a newer pick made while that save was in flight.
const AFFECTS_DISPLAY_ECHO_MS = 1000;
let affectsDisplayEchoes: { key: string; at: number }[] = [];

function noteAffectsDisplayEcho(display: AffectsDisplay): void {
  const now = Date.now();
  affectsDisplayEchoes = affectsDisplayEchoes.filter((e) => now - e.at < AFFECTS_DISPLAY_ECHO_MS);
  affectsDisplayEchoes.push({ key: JSON.stringify(normalizeAffectsDisplay(display)), at: now });
}

/** Whether an affects display heard on the bus is this window's own
 *  broadcast coming back. */
export function isOwnAffectsDisplayEcho(display: AffectsDisplay): boolean {
  const now = Date.now();
  const key = JSON.stringify(normalizeAffectsDisplay(display));
  return affectsDisplayEchoes.some((e) => e.key === key && now - e.at < AFFECTS_DISPLAY_ECHO_MS);
}

// Every window hears its own broadcast. A theme id or theme fields
// this window sent in the last second are its own echo, and adopting
// one could undo a newer pick made while that save was in flight.
const THEME_ECHO_MS = 1000;
let themeEchoes: { key: string; at: number }[] = [];

function themeEchoKey(value: string | ThemePrefs): string {
  return typeof value === 'string' ? `id:${value}` : `prefs:${JSON.stringify(themePrefsOf(value))}`;
}

function noteThemeEcho(value: string | ThemePrefs): void {
  const now = Date.now();
  themeEchoes = themeEchoes.filter((e) => now - e.at < THEME_ECHO_MS);
  themeEchoes.push({ key: themeEchoKey(value), at: now });
}

/** Whether a theme id or theme fields heard on the bus are this
 *  window's own broadcast coming back. */
export function isOwnThemeEcho(value: string | ThemePrefs): boolean {
  const now = Date.now();
  const key = themeEchoKey(value);
  return themeEchoes.some((e) => e.key === key && now - e.at < THEME_ECHO_MS);
}

/** Save the theme choice alone. A full setUiConfig from a window that
 *  is not Settings would write its stale copy of every other field.
 *  Pass the light and dark pair too when the pick came from pickTheme,
 *  which fills one of them while follow system appearance is on. */
export async function setUiTheme(
  theme: string,
  pair?: { light_theme: string; dark_theme: string },
): Promise<void> {
  await invoke('ui_set_theme', {
    theme,
    lightTheme: pair?.light_theme ?? null,
    darkTheme: pair?.dark_theme ?? null,
  });
}

/** The snake_case payload `ui_set_config` takes, matching the Rust
 *  `UiConfigPayload` DTO, the same shape `ui_get_config` returns.
 *  `dock_layout` is omitted on purpose, since only the old dock to
 *  panes conversion reads it. */
function uiConfigPayload(config: UiConfig): Record<string, unknown> {
  return {
    theme: config.theme,
    follow_system_appearance: config.follow_system_appearance,
    light_theme: config.light_theme,
    dark_theme: config.dark_theme,
    auto_update: config.auto_update,
    font_family: config.font_family,
    font_size: config.font_size,
    terminal_line_height: config.terminal_line_height,
    // Wire format intentionally drops `label: null` to the omitted
    // form so the backend's `Option<String>` deserializes cleanly.
    tracked_affects: config.tracked_affects.map((t) => ({
      name: t.name,
      ...(t.label ? { label: t.label } : {}),
    })),
    enabled_presets: config.enabled_presets,
    keep_last_command: config.keep_last_command,
    theme_terminal_colors: config.theme_terminal_colors,
    bright_bold: config.bright_bold,
    blink_text: config.blink_text,
    readable_highlights: config.readable_highlights,
    collapse_repeats: config.collapse_repeats,
    terminal_base_ansi: config.terminal_base_ansi,
    custom_themes: config.custom_themes,
    split_divider_color: config.split_divider_color,
    input_echo_color: config.input_echo_color,
    echo_macros: config.echo_macros,
    input_echo_caret: config.input_echo_caret,
    paste_line_delay_ms: config.paste_line_delay_ms,
    spellcheck_prompt: config.spellcheck_prompt,
    input_cursor_style: config.input_cursor_style,
    vitals_density: config.vitals_density,
    vitals_values: config.vitals_values,
    vitals_meter: config.vitals_meter,
    vitals_warn_thirds: config.vitals_warn_thirds,
    vitals_hide_when_pinned: config.vitals_hide_when_pinned,
    chip_style: config.chip_style,
    tick_count: config.tick_count,
    game_time: config.game_time,
    affects_style: config.affects_style,
    affects_marker: config.affects_marker,
    affects_tint: config.affects_tint,
    affects_running_out_hours: config.affects_running_out_hours,
    affects_almost_gone_hours: config.affects_almost_gone_hours,
    generation: config.generation ?? null,
  };
}

/** Save the whole config and tell every window what changed. Resolves
 *  false when the backend turned the save away, because it replaced
 *  the live config after this copy was read (a profile switch, a
 *  #profile load or reset, or an import). Nothing is sent then, and
 *  this window reads the config again. */
export async function setUiConfig(config: UiConfig): Promise<boolean> {
  const applied = await invoke<boolean | undefined>('ui_set_config', {
    config: uiConfigPayload(config),
  });
  if (applied === false) {
    // The old profile's values stay off the new one. Take the new copy,
    // even if the replace notice never reached this window.
    for (const reread of replaceFollowers) reread();
    return false;
  }
  // Theme + custom-themes go out first so any other window's theme
  // registry is current by the time `theme-changed` points at a
  // custom theme id. `broadcastUiConfigChanges` preserves that
  // ordering.
  await broadcastUiConfigChanges(config);
  return true;
}

/** Hear a new chip style saved from Settings. setUiConfig emits it to
 *  every window, so the main window's status line follows at once. */
export async function subscribeChipStyleChanged(
  cb: (value: ChipStyle) => void,
): Promise<UnlistenFn> {
  return listen<unknown>('vosh://chip-style-changed', (event) => {
    cb(normalizeChipStyle(event.payload));
  });
}

/** Hear a new tick count saved from Settings. setUiConfig emits it to
 *  every window, so the main window's status line follows at once. */
export async function subscribeTickCountChanged(
  cb: (value: TickCount) => void,
): Promise<UnlistenFn> {
  return listen<unknown>('vosh://tick-count-changed', (event) => {
    cb(normalizeTickCount(event.payload));
  });
}

/** Hear a new game time clock saved from Settings, or the one a
 *  profile switch brings. setUiConfig emits it to every window, so the
 *  main window's status line follows at once. */
export async function subscribeGameTimeChanged(cb: (value: GameTime) => void): Promise<UnlistenFn> {
  return listen<unknown>('vosh://game-time-changed', (event) => {
    cb(normalizeGameTime(event.payload));
  });
}

/** Hear a new terminal line height saved from Settings. */
export async function subscribeTerminalLineHeightChanged(
  cb: (value: TerminalLineHeight) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TERMINAL_LINE_HEIGHT_EVENT, (event) => {
    cb(normalizeTerminalLineHeight(event.payload));
  });
}

/** Hear a new vitals density saved from Settings, or the one a
 *  profile switch brings. */
export async function subscribeVitalsDensityChanged(
  cb: (value: VitalsDensity) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(VITALS_DENSITY_EVENT, (event) => {
    cb(normalizeVitalsDensity(event.payload));
  });
}

/** Hear a new affects display, saved from Settings or picked in the
 *  pane menu, or the one a profile switch brings. */
export async function subscribeAffectsDisplayChanged(
  cb: (value: AffectsDisplay) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(AFFECTS_DISPLAY_EVENT, (event) => {
    cb(normalizeAffectsDisplay(event.payload));
  });
}

/** Save an affects display pick alone, for the pane menu. A full
 *  setUiConfig from the main window would write its stale copy of every
 *  other field. The backend tells every window. */
export async function setAffectsDisplay(patch: Partial<AffectsDisplay>): Promise<void> {
  await invoke('ui_set_affects_display', {
    style: patch.style ?? null,
    marker: patch.marker ?? null,
    tint: patch.tint ?? null,
    runningOut: patch.running_out ?? null,
    almostGone: patch.almost_gone ?? null,
  });
}

/** The chat pane's channel colors for the live profile, as the backend
 *  holds them. chatColors.ts normalizeChatColors reads the table. */
export async function getChatColorsTable(): Promise<unknown> {
  return invoke<unknown>('ui_get_chat_colors');
}

/** Recolor one chat channel from the pane menu, or give it back its
 *  default with null. The backend saves it alone, so no window writes a
 *  stale copy of the rest of the config, and tells every window. */
export async function setChatColor(channel: string, color: string | null): Promise<void> {
  await invoke('ui_set_chat_color', { channel, color });
}

/** Give every chat channel its default color again. */
export async function resetChatColors(): Promise<void> {
  await invoke('ui_reset_chat_colors');
}

/** Hear the chat colors after a pick, a reset, or a profile switch. */
export async function subscribeChatColorsChanged(
  cb: (table: unknown) => void,
): Promise<UnlistenFn> {
  return listen<unknown>('vosh://chat-colors-changed', (event) => {
    cb(event.payload);
  });
}

/** Hear new vitals options (Values, Meter, Warn before you run low)
 *  saved from Settings, or the ones a profile switch brings. */
export async function subscribeVitalsOptionsChanged(
  cb: (value: VitalsOptions) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(VITALS_OPTIONS_EVENT, (event) => {
    cb(normalizeVitalsOptions(event.payload));
  });
}

/** Hear the Blinking text choice change, null for none. */
export async function subscribeBlinkTextChanged(
  cb: (value: boolean | null) => void,
): Promise<UnlistenFn> {
  return listen<boolean | null>(BLINK_TEXT_EVENT, (event) => {
    cb(typeof event.payload === 'boolean' ? event.payload : null);
  });
}

export async function subscribeBrightBoldChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>('vosh://bright-bold-changed', (event) => {
    cb(Boolean(event.payload));
  });
}

/** Hear Keep highlight colors readable change, saved in Settings or
 *  brought by another profile. */
export async function subscribeReadableHighlightsChanged(
  cb: (value: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>(READABLE_HIGHLIGHTS_EVENT, (event) => {
    cb(event.payload !== false);
  });
}

export async function subscribeSplitDividerChanged(
  cb: (color: string | null) => void,
): Promise<UnlistenFn> {
  return listen<string | null>('vosh://split-divider-changed', (event) => {
    cb(typeof event.payload === 'string' && event.payload.length > 0 ? event.payload : null);
  });
}

export async function subscribeBaseAnsiChanged(
  cb: (colors: string[] | null) => void,
): Promise<UnlistenFn> {
  return listen<unknown>('vosh://base-ansi-changed', (event) => {
    const p = event.payload;
    cb(
      Array.isArray(p) && p.length === 16 && p.every((c) => typeof c === 'string')
        ? (p as string[])
        : null,
    );
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
  /** Character names the profile claims. Any-of matching: a connect
   *  call carrying any one of these names triggers a profile switch.
   *  Empty list means the profile matches on host/port alone.
   *  The legacy single-string `character` field is accepted by the
   *  backend on load and promoted to a one-element list, so older
   *  profiles keep working without manual migration. */
  characters?: string[];
  /** The login toggle. False keeps the world and the names but stops
   *  the profile from matching at connect or login. The backend leaves
   *  it out while it is on, so absent means on. */
  enabled?: boolean;
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

/** The sentences launch kept for you, such as a profile file Vosh could
 *  not read and will not save over. The first call takes them, and every
 *  later call gets none. */
export async function launchNoticesTake(): Promise<string[]> {
  return invoke('launch_notices_take');
}

export async function profileResolveMatch(
  host: string,
  port: number,
  character: string | null,
): Promise<string | null> {
  return invoke('profile_resolve_match', { host, port, character });
}

// Characters. Settings > Characters edits any profile in place, active
// or not, and never switches the live session. A command that takes an
// optional profile acts on the live profile when you leave it out or
// name the active one, and on the inactive profile's file otherwise.
// Every edit to a profile's detail announces itself as
// vosh://profile-changed with the profile's name. An inactive edit never
// fires vosh://tracked-affects-changed or vosh://pane-layout-changed,
// which carry the live profile's data to the main window.

/** One profile as the Characters group shows it. */
export interface ProfileDetail {
  name: string;
  /** `Default` for the reserved `default` profile, else the name. */
  display_name: string;
  /** Whether this is the live profile. */
  active: boolean;
  auto_match: ProfileAutoMatch | null;
  /** The display name of the profile's world, like `The Forsaken
   *  Lands`, or null when it has no world. */
  world_name: string | null;
  tracked_affects: TrackedAffect[];
  /** The pane tree the panel shows once this profile is live. */
  panes: PaneLayout;
  /** The live pane generation for the active profile, null for an
   *  inactive one. */
  generation: number | null;
  /** Whether the login toggle reads on. True only when the profile's
   *  first character logging in on its world would load this profile,
   *  so a profile that loses a claim to another reads off. */
  login_on: boolean;
}

export async function profileDetailGet(name: string): Promise<ProfileDetail> {
  const raw = await invoke<ProfileDetail>('profile_detail_get', { name });
  return {
    ...raw,
    tracked_affects: normalizeTrackedAffects(
      Array.isArray(raw.tracked_affects) ? raw.tracked_affects : [],
    ),
    panes: sanitizeLayout(raw.panes),
  };
}

/** Replace a profile's tracked affects and get back the list as saved,
 *  trimmed and with names repeated in another case dropped. */
export async function trackedAffectsSet(
  list: TrackedAffect[],
  profile?: string | null,
): Promise<TrackedAffect[]> {
  const saved = await invoke<unknown>('tracked_affects_set', { list, profile: profile ?? null });
  return normalizeTrackedAffects(Array.isArray(saved) ? saved : []);
}

/** Put a profile's panes back to the stock map over affects tree,
 *  keeping whether its panel shows and how wide it is. Returns the new
 *  layout. The live profile saves it at once and every window hears it
 *  through vosh://pane-layout-changed. */
export async function paneLayoutReset(profile?: string | null): Promise<PaneLayout> {
  return sanitizeLayout(await invoke<unknown>('pane_layout_reset', { profile: profile ?? null }));
}

/** What turning a login toggle on or off did. */
export interface LoginClaim {
  /** The profile as it now reads. */
  entry: ProfileEntry;
  /** Every profile the character was taken from, in index order. */
  released_from: string[];
}

/** Turn the login toggle on or off for `character`. On takes the
 *  character from every other profile on the same world, since a
 *  character belongs to one profile per world. Off keeps the world and
 *  the name. Never switches the live profile. */
export async function profileSetLogin(
  name: string,
  character: string,
  on: boolean,
): Promise<LoginClaim> {
  return invoke('profile_set_login', { name, character, on });
}

/** Point a profile at a world. Edits only the host and port. A null or
 *  blank host clears the world. */
export async function profileSetWorld(
  name: string,
  host: string | null,
  port: number | null,
): Promise<ProfileEntry> {
  return invoke('profile_set_world', { name, host, port });
}

/** Create a profile, starting as a copy of `copyFrom` when given, with
 *  `autoMatch` as its login claim. The claim takes nothing from other
 *  profiles, so follow with profileSetLogin to own the character. Does
 *  not switch. */
export async function profileCreate(
  name: string,
  copyFrom?: string | null,
  autoMatch?: ProfileAutoMatch | null,
): Promise<ProfileEntry> {
  return invoke('profile_create', {
    name,
    copyFrom: copyFrom ?? null,
    autoMatch: autoMatch ?? null,
  });
}

/** Where profileExportFile saved a profile. */
export interface ProfileExport {
  path: string;
  /** Like `Ilsabet profile.toml`. */
  file_name: string;
}

/** Save a profile's settings, active or not, as a TOML file in your
 *  Downloads folder. The name never replaces a file already there. */
export async function profileExportFile(name: string): Promise<ProfileExport> {
  return invoke('profile_export_file', { name });
}

/** Who is logged in. */
export interface SessionIdentity {
  host: string;
  port: number;
  /** The character from Char.Status or Char.Name, once the MUD sends
   *  it. */
  character: string | null;
  /** The live profile. */
  profile: string;
  /** The profile that claims `character` on this world, or null when
   *  no profile does. A profile that only matches the host does not
   *  count. */
  claimed_by: string | null;
}

/** The current session identity, or null while no connection is up. */
export async function sessionIdentityGet(): Promise<SessionIdentity | null> {
  return (await invoke<SessionIdentity | null>('session_identity_get')) ?? null;
}

/** Hear the session identity change after a connect, a disconnect, or
 *  a login. */
export async function subscribeSessionIdentity(
  cb: (identity: SessionIdentity | null) => void,
): Promise<UnlistenFn> {
  return listen<SessionIdentity | null>('vosh://session-identity-changed', (event) => {
    cb(event.payload ?? null);
  });
}

/** Hear an edit to any profile's detail, active or not, by name. */
export async function subscribeProfileChanged(cb: (name: string) => void): Promise<UnlistenFn> {
  return listen<unknown>('vosh://profile-changed', (event) => {
    const payload = event.payload as { name?: unknown } | null;
    if (typeof payload?.name === 'string') cb(payload.name);
  });
}

// Path B migration preview. The backend walks the current profile set,
// loads each per-profile snapshot, and returns the merge plan: every
// auto-resolved item, every conflict (one entry per name with two or
// more diverging variants), and one derived loadout per source profile.
// Read-only — nothing is written to disk by this call. The eventual
// migration_apply (not in this build) commits the plan after the user
// picks conflict winners in the wizard.

export type MigrationItemKind = 'alias' | 'trigger' | 'macro';

export interface MigrationVariant {
  source_profile: string;
  /** Whether its profile had this copy switched on. */
  switched_on: boolean;
  item: { kind: MigrationItemKind; item: Record<string, unknown> };
}

export interface MigrationConflict {
  kind: MigrationItemKind;
  name: string;
  variants: MigrationVariant[];
  /** The profile whose version the wizard keeps unless you pick another:
   *  the one version switched on anywhere when exactly one is, or else
   *  the first profile. */
  default_source: string;
}

export interface MigrationLoadoutPreview {
  name: string;
  description?: string | null;
  enabled_groups: string[];
}

export interface MigrationPlan {
  source_profiles: string[];
  auto_resolved: {
    aliases: Array<{ name: string; group?: string | null }>;
    triggers: Array<{ name: string; group?: string | null }>;
    macros: Array<{ key: string; group?: string | null }>;
  };
  conflicts: MigrationConflict[];
  loadouts: MigrationLoadoutPreview[];
  /** The enabled preset list every character shares in loadout mode. */
  shared_presets: string[];
  /** Each source profile's own enabled preset list, in the order of
   *  source_profiles. A profile that never saved a file holds the
   *  defaults, the empty list. */
  profile_presets: string[][];
}

/** Preview the shared catalog. `library` holds the id of every preset in
 *  the library this build installs from, so a preset it no longer has
 *  stays off for characters whose file lacks it. */
export async function migrationAnalyze(library: string[]): Promise<MigrationPlan> {
  return invoke('migration_analyze', { library });
}

export interface MigrationConflictResolution {
  kind: MigrationItemKind;
  name: string;
  source_profile: string;
}

// Commit the migration. The backend writes catalog.toml + loadouts.toml
// and moves per-profile files into profiles/legacy/. Returns Ok once
// the files have landed. The runtime stays in legacy mode until the
// user relaunches Vosh; the startup hook detects catalog.toml on next
// launch and enters Path B mode. The wizard prompts the user to quit
// + reopen via appQuit because app.restart() is fragile in dev mode
// and silently leaves the WebView with no frontend to load.
export async function migrationApply(
  resolutions: MigrationConflictResolution[],
  library: string[],
): Promise<void> {
  return invoke('migration_apply', { resolutions, library });
}

/** Hear that the shared catalog wizard wrote its files. Nothing saves
 *  until Vosh opens again, and the main window says so. */
export async function subscribeMigrationApplied(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>('vosh://migration-applied', () => cb());
}

// Cleanly quit Vosh. The post-migration prompt uses this so the
// user can relaunch into Path B mode in one click.
export async function appQuit(): Promise<void> {
  return invoke('app_quit');
}

// Path B loadout state for the Settings UI. `path_b_active` is the
// flag the frontend reads to decide whether to render the Loadouts
// tab at all; in legacy mode it returns false and empty lists.
export interface LoadoutSummary {
  name: string;
  description?: string | null;
  enabled_groups: string[];
}

export interface LoadoutsState {
  path_b_active: boolean;
  active: string[];
  loadouts: LoadoutSummary[];
}

export async function loadoutsGetState(): Promise<LoadoutsState> {
  return invoke('loadouts_get_state');
}

// Replace the active-loadouts list. The backend recomputes every
// store's disabled_groups, persists the loadout set, and emits
// vosh://loadouts-changed so other consumers see the update.
export async function loadoutsSetActive(active: string[]): Promise<void> {
  return invoke('loadouts_set_active', { active });
}

export async function subscribeLoadoutsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen('vosh://loadouts-changed', () => cb());
}

// Live tick-timer configuration. Mirrors TickConfig in tick.rs on the
// backend. Optional fields use null to mean "feature off / use
// default"; the backend trims empty strings to null on write.
export interface TickConfig {
  enabled: boolean;
  interval_secs: number;
  auto_fire: string | null;
  sound: boolean;
  reset_pattern: string | null;
  warn_at_secs: number | null;
  warn_message: string | null;
  warn_color: string | null;
}

export async function tickGetConfig(): Promise<TickConfig> {
  return invoke('tick_get_config');
}

export async function tickSetConfig(config: TickConfig): Promise<TickConfig> {
  return invoke('tick_set_config', { config });
}

export async function subscribeTickConfigChanged(
  cb: (cfg: TickConfig) => void,
): Promise<UnlistenFn> {
  return listen<TickConfig>('vosh://tick-config-changed', (event) => cb(event.payload));
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
