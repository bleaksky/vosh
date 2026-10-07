// Your design as the prompt card edits it, saved as you change it, with
// Command Z to take a change back.

import { useCallback, useRef, useState } from 'react';
import type { PromptConfig } from '../ipc/prompt';
import {
  promptDescribe,
  promptEdit,
  type PromptDescribed,
  type PromptEditOp,
  type PromptPiece,
  type PromptPreviewName,
} from '../ipc/promptDesign';
import { errorText } from '../lib/text';
import { pushToast } from '../stores/toasts';
import type { CardBinding } from './cardBinding';
import { editedTable, movedBackTable, takeBackOnto, undoEntry, type UndoEntry } from './cardRules';
import { caretAfter, moveBack, moveOp, type MoveMade, type Pointing } from './promptPieces';

const UNDO_DEPTH = 50;

/** The table the card shows for `session` and the changes it makes to
 *  it, which `write` saves. The card puts a table it reads in with take, bumps opens each time
 *  it opens, and calls forgetEdits as it opens again for another profile.
 *  An edit hands the parts of the new design to `setDescribed`, in the
 *  preview `previewRef` names, and the part it follows to
 *  `setPointing`. */
export function useDesignEdits(
  session: number,
  write: CardBinding['write'],
  setDescribed: (described: { template: string; data: PromptDescribed }) => void,
  setPointing: (pointing: Pointing) => void,
  previewRef: { readonly current: PromptPreviewName },
) {
  const [config, setConfig] = useState<PromptConfig | null>(null);
  const latest = useRef<PromptConfig | null>(null);

  // take and forgetEdits keep one identity, so the card's open effect
  // can list them and still run only as the card mounts.
  const take = useCallback((next: PromptConfig) => {
    latest.current = next;
    setConfig(next);
  }, []);

  // Counts each time the card opens, for the profile active then. A
  // read or a save begun for an earlier one never lands on this one.
  const opens = useRef(0);
  const edits = useRef<Promise<unknown>>(Promise.resolve());
  const undo = useRef<UndoEntry[]>([]);
  // The moves Option with Left or Right made, newest last, which the
  // opposite key takes back exactly.
  const moves = useRef<MoveMade[]>([]);

  /** Drop the table and every change there is to take back, so nothing
   *  saves until the card reads the next table. */
  const forgetEdits = useCallback(() => {
    latest.current = null;
    setConfig(null);
    undo.current = [];
    moves.current = [];
  }, []);

  const save = (next: PromptConfig, keepUndo = true, asIs = false) => {
    const before = latest.current;
    if (!before) return;
    const at = opens.current;
    // Command Z takes back only what this change made.
    const entry = keepUndo ? undoEntry(before, next) : null;
    if (entry) undo.current = [...undo.current, entry].slice(-UNDO_DEPTH);
    take(next);
    void write(next, { asIs, session }).catch((e: unknown) => {
      if (at === opens.current) take(before);
      pushToast({ kind: 'error', message: String(e) });
    });
  };

  const takeBack = () => {
    const last = undo.current.pop();
    const now = latest.current;
    if (last && now) save(takeBackOnto(now, last), false, true);
  };

  /** Make `ops` one after another on the design as it stands, save the
   *  result once, and follow the part the first one acted on: pick it,
   *  or with `caret`, put the caret past it. `made` hears the design
   *  before and after, where that part landed, and whether the design
   *  followed the game before. Edits queue, so typing fast loses no
   *  character. */
  const edit = (
    ops: PromptEditOp[],
    follow: 'pick' | 'caret' = 'pick',
    made?: (change: {
      before: string;
      after: string;
      landed: number | null;
      mirror: boolean;
    }) => void,
  ) => {
    edits.current = edits.current.then(async () => {
      const base = latest.current;
      const at = opens.current;
      if (!base || ops.length === 0) return;
      try {
        let text = base.template;
        let landed: number | null = null;
        for (const [i, op] of ops.entries()) {
          // Later ops of a run act on the part the first one acted on:
          // text goes right after it, and a change goes to it.
          let placed = op;
          if (i > 0 && landed !== null) {
            if (op.op === 'insert_text') placed = { ...op, at: landed + 1 };
            else if ('piece' in op) placed = { ...op, piece: landed };
          }
          const result = await promptEdit(text, placed, session);
          text = result.template;
          if (i === 0 || placed.op !== 'insert_text') landed = result.piece;
        }
        // The parts of the new design come with it, so the card shows
        // the part it follows at once.
        const shown = previewRef.current;
        const data = await promptDescribe(
          text,
          shown === 'now' ? null : shown,
          null,
          session,
        ).catch(() => null);
        // Another profile became active meanwhile, so the edit was for a
        // table the card no longer shows.
        if (at !== opens.current) return;
        // The table can change meanwhile, as when you pick a place, so
        // the new design goes on it as it stands.
        const edited = editedTable(base, latest.current, text);
        if (edited) save(edited);
        if (data) setDescribed({ template: text, data });
        made?.({ before: base.template, after: text, landed, mirror: base.mirror });
        const first = ops[0];
        if (landed === null || first.op === 'remove') {
          setPointing({ picked: null, caret: caretAfter(first, null) });
        } else if (follow === 'caret') {
          setPointing({ picked: null, caret: caretAfter(first, landed) });
        } else {
          setPointing({ picked: landed, caret: null });
        }
      } catch (e) {
        pushToast({ kind: 'error', message: errorText(e) });
      }
    });
  };

  /** Put the design back as it was before move `back`, following the
   *  game again when it did then, with the part it moved picked where it
   *  was. Only while the design is still what the move made. */
  const takeMoveBack = (back: MoveMade) => {
    edits.current = edits.current.then(async () => {
      const base = latest.current;
      const at = opens.current;
      if (!base || base.template !== back.after) return;
      const shown = previewRef.current;
      const data = await promptDescribe(
        back.before,
        shown === 'now' ? null : shown,
        null,
        session,
      ).catch(() => null);
      if (at !== opens.current) return;
      save(movedBackTable(base, latest.current, back));
      if (data) setDescribed({ template: back.before, data });
      setPointing({ picked: back.from, caret: null });
    });
  };

  /** Option with Left or Right: take the last move back when the key goes
   *  the other way, or move the part you picked past its neighbor among
   *  `pieces`. */
  const movePicked = (pieces: readonly PromptPiece[], picked: number | null, dir: -1 | 1) => {
    const back = moveBack(moves.current, latest.current?.template ?? '', picked, dir);
    if (back) {
      moves.current = moves.current.slice(0, -1);
      takeMoveBack(back);
      return;
    }
    const op = moveOp(pieces, picked, dir);
    if (!op || op.op !== 'move') return;
    edit([op], 'pick', ({ before, after, landed, mirror }) => {
      if (landed === null || before === after) return;
      moves.current = [
        ...moves.current,
        { before, after, from: op.piece, landed, dir, mirror },
      ].slice(-UNDO_DEPTH);
    });
  };

  return { config, take, opens, forgetEdits, save, takeBack, edit, movePicked };
}
