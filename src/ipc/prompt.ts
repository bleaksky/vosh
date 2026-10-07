// Your prompt table and the card's calls, how Vosh reads your prompt, and
// the prompt events the session sends.

import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  GAME_PROMPT_SEEN,
  HIDDEN,
  PROMPT_CARD_OPEN,
  PROMPT_CONFIG_CHANGED,
  PROMPT_GAG_WITHOUT_READER,
  PROMPT_STATE,
  PROMPT_STATUS,
  PROMPT_VARS,
} from './events';
import type { PromptSpan } from './promptDesign';
import { sessionOf, type SessionData } from './session';

// Prompt vars, the values a trigger writes with
// `mud.set_prompt_var(...)`. The vitals store reads them with priority
// over GMCP, so a #prompt capture can feed hp, mana and moves from the
// prompt text. The payload's data is the full snapshot, and the
// frontend replaces its copy. A value for a name GMCP also supplies,
// such as hp, drops out at the next Char.Vitals, or at your next send
// on a server without it. A value the game hides comes as `?`. The
// listener gets the session that sent them too.
export type PromptVarsPayload = Record<string, string>;

export async function onPromptVars(
  cb: (payload: PromptVarsPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<SessionData<PromptVarsPayload>>(PROMPT_VARS, (event) => {
    cb(event.payload.data, sessionOf(event.payload));
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

/** Hear each report, with the session it is about. */
export async function onHidden(
  cb: (payload: HiddenPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<HiddenPayload & { session?: number }>(HIDDEN, (event) => {
    cb(event.payload, sessionOf(event.payload));
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
  cb: (payload: GamePromptSeenPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<GamePromptSeenPayload & { session?: number }>(GAME_PROMPT_SEEN, (event) => {
    const { kind, text, applied, lost } = event.payload as GamePromptSeenPayload & {
      lost?: unknown;
    };
    cb(
      {
        kind,
        text,
        applied,
        lost: Array.isArray(lost) ? lost.filter((n): n is string => typeof n === 'string') : [],
      },
      sessionOf(event.payload),
    );
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
export async function promptLastSeen(session?: number): Promise<PromptLastSeen | null> {
  return invoke('prompt_last_seen', { session });
}

/** What the backend last reported on session://hidden for a session,
 *  for a window that opens or reloads after the report. */
export async function hiddenGet(session: number): Promise<HiddenPayload> {
  return invoke('hidden_get', { session });
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

/** The `[prompt]` table of a session, the selected one when it names
 *  none. */
export async function promptConfigGet(session?: number): Promise<PromptConfig> {
  return normalizePromptConfig(
    await invoke<RawPromptConfig | null>('prompt_config_get', { session }),
  );
}

/** Tell the backend the card opened, which keeps the design it found
 *  among the earlier designs, and read the table as it now stands. */
export async function promptCardOpen(session?: number): Promise<PromptConfig> {
  return normalizePromptConfig(
    await invoke<RawPromptConfig | null>('prompt_card_open', { session }),
  );
}

/** What the main window opens the card on: where it would open, Edit as
 *  text, or pointing at the line your game prints, which Point at it
 *  again… in Settings asks for. */
export type PromptCardView = 'text' | 'point' | null;

export interface PromptCardRequest {
  view: PromptCardView;
  /** The card for your vitals text, Your vitals text, in place of your
   *  prompt's. */
  vitals?: true;
}

/** Ask the main window to open the prompt card, from any window, such as
 *  Customize… in Settings. */
export async function openPromptCard(view: PromptCardView = null): Promise<void> {
  await emit(PROMPT_CARD_OPEN, { view });
}

/** Ask the main window to open the card for your vitals text, from Edit…
 *  in Settings or Edit your text… on the vitals menu. */
export async function openVitalsTextCard(): Promise<void> {
  await emit(PROMPT_CARD_OPEN, { view: null, vitals: true });
}

/** Hear a window ask for the prompt card. */
export async function subscribePromptCardOpen(
  cb: (request: PromptCardRequest) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PROMPT_CARD_OPEN, (event) => {
    const raw = event.payload as { view?: unknown; vitals?: unknown } | null;
    const view = raw?.view;
    const known = view === 'text' || view === 'point' ? view : null;
    cb(raw?.vitals === true ? { view: known, vitals: true } : { view: known });
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
  options?: { asIs?: boolean; session?: number | undefined },
): Promise<void> {
  const session = options?.session;
  await invoke(
    'prompt_config_set',
    options?.asIs ? { config, asIs: true, session } : { config, session },
  );
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
export async function promptCompile(
  capture: PromptCompileRequest,
  session?: number,
): Promise<PromptCompileReport> {
  return invoke('prompt_compile', { capture, session });
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
export async function promptCandidates(session?: number): Promise<PromptCandidateGroup[]> {
  return invoke('prompt_candidates', { session });
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
  session?: number,
): Promise<PromptCompileReport> {
  return invoke('prompt_capture_from_line', { id, names: names ?? null, session });
}

/** Check a capture against the candidates ring and your scrollback. */
export async function promptCaptureCheck(
  capture: PromptCapture,
  session?: number,
): Promise<PromptCaptureCheck> {
  return invoke('prompt_capture_check', { capture, session });
}

/** The open card chose Aabahran's code reader on a host Vosh does not
 *  know (More > Use Forsaken Lands prompt codes…), or lets it go. While
 *  it holds, the Forsaken Lands rules hold, so the game's reply to prompt
 *  fills the card's fields on an older build (D17). */
export async function promptCodeReaderSet(on: boolean, session?: number): Promise<void> {
  await invoke('prompt_code_reader_set', { on, session });
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
export async function promptLineTriggers(
  capture: PromptCapture,
  session?: number,
): Promise<PromptLineTrigger[]> {
  return invoke('prompt_line_triggers', { capture, session });
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
export async function promptStateGet(session?: number): Promise<PromptState> {
  return invoke('prompt_state_get', { session });
}

/** While on, session://prompt-state follows each prompt Vosh reads in a
 *  session. */
export async function promptWatch(on: boolean, session?: number): Promise<void> {
  await invoke('prompt_watch', { on, session });
}

/** The prompt state after each prompt, while the card watches, with the
 *  session it is about. */
export async function onPromptState(
  cb: (payload: PromptState, session: number) => void,
): Promise<UnlistenFn> {
  return listen<PromptState & { session?: number }>(PROMPT_STATE, (event) => {
    cb(event.payload, sessionOf(event.payload));
  });
}

/** Whether Vosh reads your prompt, each time that changes, with the
 *  session it is about. */
export async function onPromptStatus(
  cb: (payload: PromptStatusPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<PromptStatusPayload & { session?: number }>(PROMPT_STATUS, (event) => {
    const { status, last_match_at } = event.payload;
    cb({ status, last_match_at }, sessionOf(event.payload));
  });
}

/** A trigger hid your prompt and set prompt values while this profile
 *  draws nothing in its place, once per trigger per session. The
 *  listener gets the session too. */
export async function onPromptGagWithoutReader(
  cb: (payload: { trigger: string }, session: number) => void,
): Promise<UnlistenFn> {
  return listen<{ trigger: string; session?: number }>(PROMPT_GAG_WITHOUT_READER, (event) => {
    cb({ trigger: event.payload.trigger }, sessionOf(event.payload));
  });
}

/** The triggers that hid your prompt in a session with nothing drawn in
 *  its place, for a window that opens after the session named them. */
export async function promptGagsWithoutReader(session?: number): Promise<string[]> {
  return invoke('prompt_gags_without_reader', { session });
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

export interface PromptShowState {
  show: PromptShow;
  /** The profile has a capture that reads a prompt. */
  capture: boolean;
  /** Draw your prompt is on. */
  draw: boolean;
  /** The game sent Char.Prompt this session. */
  gameSent: boolean;
  /** The rows the pinned band keeps, the most any prompt the capture
   *  reads can take. */
  zone: number;
  /** You turned prompts off in the game. */
  promptsOff: boolean;
}

interface RawPromptShowState {
  show?: unknown;
  capture?: unknown;
  draw?: unknown;
  game_sent?: unknown;
  zone?: unknown;
  prompts_off?: unknown;
}

/** A state from what prompt_show_get returned, the text and nothing
 *  read for anything it did not say. */
export function normalizePromptShowState(raw: RawPromptShowState | null): PromptShowState {
  const zone = typeof raw?.zone === 'number' && Number.isFinite(raw.zone) ? raw.zone : 1;
  return {
    show: normalizePromptShow(raw?.show),
    capture: raw?.capture === true,
    draw: raw?.draw === true,
    gameSent: raw?.game_sent === true,
    zone: Math.min(6, Math.max(1, Math.round(zone))),
    promptsOff: raw?.prompts_off === true,
  };
}

/** Where a session's prompt shows, the selected session's when it
 *  names none. */
export async function promptShowGet(session?: number): Promise<PromptShowState> {
  return normalizePromptShowState(
    await invoke<RawPromptShowState | null>('prompt_show_get', { session }),
  );
}

/** The payload of vosh://prompt-config-changed: the active profile
 *  whose table changed, null before any profile loads. */
export interface PromptConfigChangedPayload {
  profile: string | null;
}

/** Hear that the active profile's `[prompt]` table changed, by
 *  `#prompt`, `#unprompt`, the card or a Settings save. */
export async function subscribePromptConfigChanged(
  cb: (payload: PromptConfigChangedPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PROMPT_CONFIG_CHANGED, (event) => {
    const raw = event.payload as { profile?: unknown } | null;
    cb({ profile: typeof raw?.profile === 'string' ? raw.profile : null });
  });
}
