import type { ReactNode } from 'react';
import { CheckIcon } from '../ui';
import type { Note } from './words';

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
      <span className={`wr-note-dot is-${note.tone}`} aria-hidden="true" />
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
