import type { ReactNode } from 'react';
import { Button, CheckIcon } from '../ui';
import type { FootAction, FootButton, FootLeft } from './cardFoot';
import type { Note } from './words';

/** The dot for each tone of a note that is not a plain ok. */
const NOTE_DOT: Record<Exclude<Note['tone'], 'ok'>, string> = {
  bad: 'is-danger',
  warn: 'is-warn',
  info: 'is-accent',
};

// The writing card's footer: on the left the count, or what is wrong
// with the line the caret is on, or what a send left, and on the right
// the fix beside the card's main button.

export function FootNote({ note }: { note: Note }) {
  if (note.tone === 'ok') {
    return (
      <span className="pc-foot-note wr-count wr-c-ok">
        <CheckIcon size={12} />
        <span>{note.lead + note.rest}</span>
      </span>
    );
  }
  return (
    <span className="wr-note">
      <span className={`wr-note-dot dot ${NOTE_DOT[note.tone]}`} aria-hidden="true" />
      <span className="wr-note-msg">
        {note.lead && <b>{note.lead}</b>}
        {note.rest}
      </span>
    </span>
  );
}

export function FootCount({
  main,
  tone,
  rest,
}: {
  main: string;
  tone: 'n' | 'warn' | 'bad';
  rest: string;
}) {
  return (
    <span className="pc-foot-note wr-count">
      <span className={`wr-c-${tone}`}>{main}</span>
      <span className="wr-c-of">{rest}</span>
    </span>
  );
}

export function WritingFoot({ left, right }: { left: ReactNode; right: ReactNode }) {
  return (
    <div className="pc-foot wr-foot">
      {left}
      <div className="pc-foot-end">{right}</div>
    </div>
  );
}

/** The left of the footer: a job's progress, a note or the count. */
export function FootLeftSide({ left }: { left: FootLeft }) {
  return 'progress' in left ? (
    <span className="pc-foot-note">{left.progress}</span>
  ) : 'note' in left ? (
    <FootNote note={left.note} />
  ) : (
    <FootCount {...left.count} />
  );
}

/** The footer's buttons, each running its action. */
export function FootButtons({
  buttons,
  actions,
}: {
  buttons: readonly FootButton[];
  actions: Record<FootAction, () => void>;
}) {
  return buttons.map((b) => (
    <Button
      key={b.id}
      variant={b.primary ? 'primary' : 'secondary'}
      disabled={b.disabled === true}
      onClick={actions[b.id]}
    >
      {b.label}
    </Button>
  ));
}
