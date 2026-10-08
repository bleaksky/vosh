import type { Draft, JobResult, ShownNote, WriteJob, WritingKind } from '../ipc/writing';
import { afterDrop, type Drop, type Find } from './cardDrop';
import {
  changedAsk,
  checkedNote,
  checkHeld,
  postedNote,
  sameNoteAsk,
  sentNote,
  type Ask,
} from './cardDialogs';
import type { Ended } from './cardFoot';
import {
  characterOf,
  checkWaits,
  getWritingFile,
  keepCharacter,
  withCheckWaiting,
  type World,
} from './draftsStore';
import type { FieldName } from './WritingFields';
import { KINDS } from './kinds';
import { sameLines } from './text';
import type { JobSpec } from './useWritingJob';
import { resultNote } from './words';

// What the writing card does when a job in the session's writer ends:
// the steps after a dropped link, a text read from the game, a send or
// a post that went through, a check, a note the game already held, and
// a refusal or a stop, each with the foot it leaves.

/** What a job's end reads from the writing card and changes on it. */
export interface JobCard {
  find: Find | null;
  lines: string[];
  kind: WritingKind;
  draft: Draft;
  world: World | null;
  name: string | null;
  openDraft: (k: WritingKind) => Draft;
  switchTo: (k: WritingKind, d?: Draft) => void;
  show: (lines: readonly string[]) => void;
  keep: (next: Draft) => void;
  markPosted: () => void;
  run: (spec: JobSpec) => void;
  saveShown: (k: WritingKind, note: ShownNote) => void;
  setBadField: (field: FieldName | null) => void;
  setDropped: (drop: Drop | null) => void;
  setFind: (find: Find | null) => void;
  setEnded: (ended: Ended | null) => void;
  setReadNow: (on: boolean) => void;
  setAdopt: (on: boolean) => void;
  setConfirm: (confirm: Ask & { run: () => void }) => void;
  setPhase: (phase: 'sent' | 'checked') => void;
}

/** Take the end of `job` with `result` on the card. */
export function endJob(result: JobResult, job: WriteJob, card: JobCard) {
  const {
    find,
    lines,
    kind,
    draft,
    world,
    name,
    openDraft,
    switchTo,
    show,
    keep,
    markPosted,
    run,
    saveShown,
    setBadField,
    setDropped,
    setFind,
    setEnded,
    setReadNow,
    setAdopt,
    setConfirm,
    setPhase,
  } = card;
  setBadField(null);
  setDropped(null);
  const k = job.kind;
  const drop = afterDrop(find, result, job, k, lines.length);
  if (drop) {
    setDropped(drop.dropped);
    setFind(drop.find);
    setEnded(drop.ended);
    if (drop.posted) markPosted();
    return;
  }
  switch (result.kind) {
    case 'read': {
      const note = result.note;
      const text = note ? note.lines : result.lines;
      const next: Draft = {
        ...(k === kind ? draft : openDraft(k)),
        kind: k,
        text,
        game: KINDS[k].board ? null : text,
        ...(note ? { to: note.to, subject: note.subject, language: note.language } : {}),
      };
      if (k !== kind) switchTo(k, next);
      else show(text);
      keep(next);
      if (result.beast && world && name) {
        const c = characterOf(getWritingFile(), world, name);
        if (c.beast !== result.beast) keepCharacter({ ...c, beast: result.beast });
      }
      setReadNow(true);
      setAdopt(note !== null);
      setEnded(null);
      return;
    }
    case 'changed':
      keep({ ...draft, game: result.lines });
      setConfirm({ ...changedAsk(k), run: () => run({ ...job, base: null }) });
      return;
    case 'sent': {
      keep({ ...draft, game: result.lines });
      setPhase('sent');
      const waits =
        world && name ? checkWaits(characterOf(getWritingFile(), world, name), k) : false;
      setEnded({
        note: sentNote(
          result.lines.length,
          sameLines(
            result.lines,
            lines.map((l) => l.replace(/"/g, "'")),
          ),
          waits ? k : null,
        ),
        actions: [],
      });
      return;
    }
    case 'posted':
      markPosted();
      setEnded({ note: postedNote(k, result.forum, result.vote), actions: [] });
      return;
    case 'checked':
      // The game holds a check that went through, or one it already
      // had, until the immortals decide. Only a decided one leaves none.
      if (world && name) {
        const c = characterOf(getWritingFile(), world, name);
        const next = withCheckWaiting(c, k, checkHeld(result.lines));
        if (next !== c) keepCharacter(next);
      }
      setPhase('checked');
      setEnded({ note: checkedNote(k, result.lines), actions: [] });
      return;
    case 'same_note':
      setConfirm({
        ...sameNoteAsk(k, result.note.subject),
        run: () => {
          saveShown(k, result.note);
          run({ ...job, clear_first: true });
        },
      });
      return;
    case 'other_note':
      if (result.board && result.note) saveShown(result.board, result.note);
      setEnded({
        note: resultNote(result, k) ?? { lead: '', rest: '', tone: 'warn' },
        actions: result.board ? ['clear-other'] : [],
        other: result.board,
      });
      return;
    case 'cleared':
      setEnded(null);
      return;
    case 'refused':
      if (result.field === 'to' || result.field === 'subject' || result.field === 'language') {
        setBadField(result.field);
      }
      setEnded({
        note: resultNote(result, k)!,
        actions: result.field === 'post' ? ['done'] : [],
      });
      return;
    case 'stopped':
      setEnded({
        note: resultNote(result, k)!,
        actions: KINDS[k].board ? [] : ['restore', 'again'],
      });
      return;
    default: {
      const note = resultNote(result, k);
      if (note) setEnded({ note, actions: [] });
    }
  }
}
