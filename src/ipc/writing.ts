// Calls and events of the writing card. A session's writer reads a text
// from the game, sends one through the game's line editor, posts a note
// or sends a text for its review, and says where it stands on
// session://writing.

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
export type WritingAction = 'read' | 'send' | 'post' | 'check' | 'clear' | 'paste';

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
  | { kind: 'dropped'; sent: number; posted: boolean }
  | { kind: 'offer_gone' };

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
