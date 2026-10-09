// What Settings, Input shows in its Prompt section: which form the game
// prompt block takes, the meta under it, the Draw your own prompt row's
// sentence, and the preview's height and choices. Pure, so the section
// stays about layout.

import { clockTime, lastSeenLine } from './cardRules';
import { parseSgrCells, type Cell } from '../terminal/sgrCells';
import type {
  PromptCapture,
  PromptCaptureCheck,
  PromptCompileReport,
  PromptLastSeen,
} from '../ipc/prompt';
import type { PromptPreviewName } from '../ipc/promptDesign';
import type { GamePromptSeen } from '../stores/gmcp/gamePromptStore';

/** How the game prompt block reads your prompt.
 *  - `codes`: the codes the game sent this session, as text.
 *  - `fields`: your codes in fields, on The Forsaken Lands while the game
 *    sent none this session.
 *  - `line`: the line you pointed at, for a pattern.
 *  - `point`: another game before you point at its line. */
export type GameBlock = 'codes' | 'fields' | 'line' | 'point';

export function gameBlock(input: {
  forsaken: boolean;
  gameSent: boolean;
  capture: PromptCapture;
}): GameBlock {
  const { forsaken, gameSent, capture } = input;
  if (forsaken) {
    if (gameSent) return 'codes';
    // The pattern your old capture trigger left waits for your codes.
    if (capture.kind === 'regex' && capture.source !== 'migrated') return 'line';
    return 'fields';
  }
  return capture.kind === 'regex' ? 'line' : 'point';
}

/** The codes the game sent this session, with when they came. */
export interface GameCodes {
  prompt: string;
  fprompt: string;
  enabled: boolean;
  /** Milliseconds since the epoch. */
  at: number;
  /** The first Char.Prompt since you connected, which the game sends at
   *  login. */
  atLogin: boolean;
}

/** The codes from the store while it holds a packet, else the latest
 *  Char.Prompt the backend kept, for a window that opened after it came.
 *  Null when the game sent none this session. */
export function gameCodesOf(
  game: GamePromptSeen | null,
  seen: PromptLastSeen | null,
): GameCodes | null {
  if (game) {
    return {
      prompt: game.prompt,
      fprompt: game.fprompt,
      enabled: game.enabled,
      at: game.receivedAt,
      atLogin: game.atLogin,
    };
  }
  if (seen?.source !== 'gmcp' || seen.prompt === null) return null;
  const at = seen.at ? new Date(seen.at).getTime() : Number.NaN;
  return {
    prompt: seen.prompt,
    fprompt: seen.fprompt ?? '',
    enabled: seen.enabled !== false,
    at: Number.isNaN(at) ? Date.now() : at,
    atLogin: seen.at_login,
  };
}

/** The world your prompt comes from, for the copy: The Forsaken Lands
 *  wherever its rules hold, else the host you play, or null with none. */
export function promptWorld(input: { forsaken: boolean; host: string }): string | null {
  if (input.forsaken) return 'The Forsaken Lands';
  const host = input.host.trim();
  return host.length > 0 ? host : null;
}

/** The sentence under "Your game's prompt" on The Forsaken Lands. */
export function gameDescription(world: string | null): string {
  return `Your prompt setting in ${world ?? 'the game'}. Vosh reads its codes.`;
}

/** The sentence under "Your game's prompt" on another game before you
 *  point at its line. */
export const POINT_DESCRIPTION = 'Point at it in Customize prompt and Vosh reads its numbers.';

/** The sentence while you have prompts off in the game. */
export const PROMPTS_OFF =
  'You turned prompts off in the game. Type prompt in the game to turn them back on.';

/** The sentence once three prompts in a row came that Vosh could not
 *  read, with when it last read one, as `#prompt` says it. */
export function notMatchingLine(lastMatchAt: string | null): string {
  const at = lastMatchAt ? new Date(lastMatchAt) : null;
  const since = at && !Number.isNaN(at.getTime()) ? clockTime(at) : 'you connected';
  return `No prompt has matched since ${since}. If you changed it in the game, point at it again.`;
}

/** The meta under the codes. */
export interface CodesMeta {
  tone: 'normal' | 'warn';
  text: string;
  /** The commands that fix codes that run together, each with Copy.
   *  Vosh never sends them. */
  fixes: string[];
}

/** The match count Settings gives: how many of your last prompts the
 *  capture matched, or why it matched none. Null before the first
 *  prompt. */
export function settingsMatchLine(check: PromptCaptureCheck | null): string | null {
  if (!check || check.total === 0) return null;
  if (check.matched === 0) return check.text;
  return check.matched === 1
    ? 'Matches your last prompt.'
    : `Matches your last ${check.matched} prompts.`;
}

const sameCodes = (a: string, b: string) => a.trimEnd() === b.trimEnd();

/** What the meta says under the codes or the fields: prompts off, no
 *  prompt matching, a warning with the commands that fix it, or where the
 *  codes came from and how they match. */
export function codesMeta(input: {
  block: 'codes' | 'fields';
  game: GameCodes | null;
  seen: PromptLastSeen | null;
  capture: PromptCapture;
  check: PromptCaptureCheck | null;
  report: PromptCompileReport | null;
  promptsOff: boolean;
  /** The not matching sentence while no prompt has matched, else null. */
  notMatching?: string | null;
  now: Date;
}): CodesMeta {
  const { block, game, seen, capture, check, report, promptsOff, now } = input;
  if (promptsOff) return { tone: 'warn', text: PROMPTS_OFF, fixes: [] };
  if (input.notMatching) return { tone: 'warn', text: input.notMatching, fixes: [] };
  if (report?.error) return { tone: 'warn', text: report.error.message, fixes: [] };
  const reads = capture.kind !== 'none';
  // Codes that run together need fixing once Vosh reads them. Before
  // that the block says only where they came from.
  const together = reads ? report?.warnings.find((w) => w.kind === 'run_together') : undefined;
  if (together) return { tone: 'warn', text: together.message, fixes: report?.fixes ?? [] };

  let source: string | null = null;
  if (block === 'codes' && game) {
    source = game.atLogin
      ? 'The game sent it when you logged in.'
      : `The game sent it at ${clockTime(new Date(game.at))}.`;
  } else if (block === 'fields') {
    const shown = capture.kind === 'aabahran' ? capture.prompt : '';
    if (seen?.prompt && seen.source !== 'gmcp' && reads && sameCodes(seen.prompt, shown)) {
      source = lastSeenLine(seen, now);
    } else if (!reads) {
      source = 'Type prompt in the game and Vosh reads the answer.';
    }
  }
  const match = reads ? settingsMatchLine(check) : null;
  const text = [source, match].filter((s): s is string => s !== null).join(' ');
  return { tone: 'normal', text, fixes: [] };
}

/** When Vosh last read the line another game prints, `Last read at
 *  8:42`, or null before it read one. */
export function lastReadLine(lastMatchAt: string | null): string | null {
  if (!lastMatchAt) return null;
  const at = new Date(lastMatchAt);
  if (Number.isNaN(at.getTime())) return null;
  return `Last read at ${clockTime(at)}`;
}

/** The sentence under Draw your own prompt: what it waits on without a
 *  capture, or what it replaces, on or off, once it has a capture. */
export function drawDescription(input: {
  capture: boolean;
  gameSent: boolean;
  world: string | null;
}): string {
  const { capture, gameSent, world } = input;
  if (!capture) {
    return gameSent ? 'Customize your prompt first.' : "Tell Vosh your game's prompt first.";
  }
  return `It takes the place of the prompt ${world ?? 'the game'} sends.`;
}

/** The preview output: 28 tall for one line, 17.5 more for each line
 *  past it. */
export function previewHeight(rows: number): number {
  return 28 + 17.5 * Math.max(0, rows - 1);
}

/** The previews Settings and the foot of Customize prompt offer. Lament
 *  only under the Forsaken Lands rules, where the game hides your values
 *  under lamented tears. */
export function previewOptions(forsaken: boolean): { value: PromptPreviewName; label: string }[] {
  const options: { value: PromptPreviewName; label: string }[] = [
    { value: 'now', label: 'Now' },
    { value: 'low_health', label: 'Low health' },
    { value: 'fight', label: 'Fight' },
  ];
  if (forsaken) options.push({ value: 'lament', label: 'Lament' });
  return options;
}

/** The preview Settings and the foot of Customize prompt draw for the
 *  one you picked: Now while Lament is your pick and the Forsaken Lands
 *  rules are away. Your pick stays, so Lament comes back with them. */
export function shownPreview(preview: PromptPreviewName, forsaken: boolean): PromptPreviewName {
  return previewOptions(forsaken).some((o) => o.value === preview) ? preview : 'now';
}

/** The meta under the preview: where to change your design, or that it
 *  draws sample values while you are offline. */
export function previewMeta(connected: boolean): string {
  return connected
    ? 'Right click your prompt in the terminal to change it there.'
    : 'Sample values until you connect.';
}

/** A drawn design as rows of cells for the preview, its trailing empty
 *  row left out. */
export function previewRows(ansi: string | null): Cell[][] {
  if (!ansi) return [[]];
  const rows = parseSgrCells(ansi);
  while (rows.length > 1 && rows[rows.length - 1].length === 0) rows.pop();
  return rows;
}
