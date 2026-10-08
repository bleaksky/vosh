// Calls and events of the writing card. A session's writer reads a text
// from the game, sends one through the game's line editor, posts a note
// or sends a text for its review, and says where it stands on
// session://writing. writing.toml keeps your drafts and posts for each
// character.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { WRITING } from './events';
import { sessionOf } from './session';

/** A text the game's editor holds, `Kind` in
 *  src-tauri/src/session/writer/kinds.rs. */
export type WritingKind =
  | 'description'
  | 'beast'
  | 'history'
  | 'personality'
  | 'purpose'
  | 'note'
  | 'journal'
  | 'application'
  | 'idea'
  | 'bug'
  | 'typo'
  | 'news'
  | 'changes'
  | 'penalty';

/** What a job asks of the game, `Action` in payloads.rs. */
export type WritingAction =
  | 'read'
  | 'send'
  | 'post'
  | 'check'
  | 'clear'
  | 'paste'
  /** Look for your note on the board's list after a drop. */
  | 'find';

/** A job for the writer, `WriteJob` in payloads.rs. */
export interface WriteJob {
  /** The page's number for the job, which its end carries. */
  id: number;
  kind: WritingKind;
  action: WritingAction;
  /** The text, one line each, codes as you type them. */
  lines?: string[];
  to?: string;
  subject?: string;
  language?: string | null;
  /** The game's copy your draft began from. A send that finds another
   *  text in the game stops to ask. */
  base?: string[] | null;
  /** The note the game holds is this one, so a post goes on. */
  adopt?: boolean;
  /** Clear the note the game holds first, which you agreed to. */
  clear_first?: boolean;
  /** Your character's name, which the game's show puts first. */
  name?: string | null;
  /** Trust 55 and up, where the game keeps a code anywhere in a line. */
  immortal?: boolean;
  /** For a find, how many notes of yours with this subject the board
   *  listed just before the post. Only more than that counts as found. */
  baseline?: number | null;
}

/** A note as the game's show prints it, `ShownNote` in game_text.rs. */
export interface ShownNote {
  subject: string;
  to: string;
  language: string | null;
  lines: string[];
}

/** How a job ended, `JobResult` in payloads.rs. */
export type JobResult =
  | { kind: 'read'; lines: string[]; beast: string | null; note: ShownNote | null }
  | { kind: 'changed'; lines: string[] }
  | { kind: 'sent'; lines: string[]; restore: string[] | null }
  | { kind: 'posted'; forum: boolean; vote: boolean }
  | { kind: 'checked'; lines: string[] }
  | { kind: 'cleared' }
  | { kind: 'pasted' }
  | {
      kind: 'refused';
      field: 'to' | 'subject' | 'language' | 'editor' | 'post';
      line: string;
    }
  | { kind: 'same_note'; note: ShownNote }
  | { kind: 'other_note'; board: WritingKind | null; note: ShownNote | null }
  | { kind: 'busy' }
  | { kind: 'too_long'; sent: number }
  | {
      kind: 'failed';
      why: 'mend' | 'no_mark' | 'bad_dot' | 'closed' | 'silent' | 'unread' | 'differs';
      line: number | null;
    }
  | { kind: 'stopped'; sent: number }
  /** `baseline` counts your notes with this subject the board listed
   *  just before the post, when Vosh could read the list. */
  | { kind: 'dropped'; sent: number; posted: boolean; baseline?: number | null }
  | { kind: 'offer_gone' }
  /** The board lists more notes of yours with the draft's subject than
   *  before the post, the last as `number`. */
  | { kind: 'found'; number: number }
  | { kind: 'not_found' }
  /** The board's list is one you cannot read. */
  | { kind: 'cant_tell' };

/** Where the game takes what you send. */
export type GameInput = 'unknown' | 'prompt' | 'editor' | 'pager';

/** Where a job stands, `JobProgress` in payloads.rs. */
export interface JobProgress {
  id: number;
  kind: WritingKind;
  action: WritingAction;
  stage:
    | 'waiting'
    | 'reading'
    | 'fields'
    | 'opening'
    | 'sending'
    | 'checking'
    | 'closing'
    | 'posting';
  /** Lines of your text the game took. */
  sent: number;
  total: number;
}

/** What a session's writer says, `WritingState` in writer.rs. */
export interface WritingState {
  game: GameInput;
  /** The text the game's editor holds, while Vosh can name it. */
  editor: WritingKind | null;
  /** The card's offer, after you opened the editor yourself. */
  offer: { id: number; kind: WritingKind } | null;
  job: JobProgress | null;
  /** Sends of the session that wait for the job. */
  held: number;
  /** How the last job ended. */
  done: { id: number; result: JobResult } | null;
}

/** Start a job in a session's writer. It starts at the game's prompt. */
export async function writingStart(job: WriteJob, session?: number): Promise<void> {
  await invoke('writing_start', { job, session });
}

/** Stop the job under way in a session. */
export async function writingStop(session?: number): Promise<void> {
  await invoke('writing_stop', { session });
}

/** Take the offer `id`, so the card opens on what the game listed. */
export async function writingTake(id: number, session?: number): Promise<void> {
  await invoke('writing_take', { id, session });
}

/** Hear each change to where a session's writer stands. */
export async function onWriting(
  cb: (state: WritingState, session: number) => void,
): Promise<UnlistenFn> {
  return listen<WritingState & { session?: number }>(WRITING, (event) => {
    const { session: _session, ...state } = event.payload;
    cb(state as WritingState, sessionOf(event.payload));
  });
}

/** A draft or a post, `Draft` in src-tauri/src/writing.rs. */
export interface Draft {
  id: string;
  kind: WritingKind;
  to?: string;
  subject?: string;
  language?: string | null;
  /** An application for a custom race, kept to 70 a line. */
  custom_race?: boolean;
  /** The room a bug or typo report began in. */
  room?: string | null;
  text: string[];
  /** The game's copy of a text it saves in place. */
  game?: string[] | null;
  /** When it last changed, or when it posted, in Unix ms. */
  at: number;
}

/** One character's writing, `Character` in writing.rs. */
export interface WritingCharacter {
  host: string;
  port: number;
  name: string;
  race?: string | null;
  level?: number | null;
  beast?: string | null;
  drafts: Draft[];
  sent: Draft[];
}

/** writing.toml, `WritingFile` in writing.rs. */
export interface WritingFile {
  version: number;
  /** The card checks spelling. */
  spelling: boolean;
  /** The guide shows beside the text. */
  guide: boolean;
  characters: Record<string, WritingCharacter>;
}

/** Every character's drafts and posts, with the card's switches. */
export async function writingFileGet(): Promise<WritingFile> {
  return invoke('writing_file_get');
}

/** Keep one character's drafts and posts. A character with nothing left
 *  leaves the file. */
export async function writingCharacterSet(character: WritingCharacter): Promise<void> {
  await invoke('writing_character_set', { character });
}

/** Keep the card's Check spelling and whether its guide shows. */
export async function writingSwitchesSet(spelling: boolean, guide: boolean): Promise<void> {
  await invoke('writing_switches_set', { spelling, guide });
}
