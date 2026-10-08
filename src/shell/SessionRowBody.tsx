import type { ReactNode } from 'react';
import type { SessionRow } from '../ipc/session';
import { sessionLabel } from '../lib/sessionLabel';
import { MARK_WORDS, type RowMark } from '../stores/session/sessionRowStore';
import { useLiveSnoops } from '../stores/session/snoopStore';
import { HandIcon, SpinnerIcon, TriangleIcon } from '../ui/icons';
import { EyeIcon } from './icons';
import { useSessionLine, type SessionLine } from './sessionLine';

// The two lines of a session's row, which the sessions sidebar and the
// session popover's list both draw (S1 and S7 of the Sessions Sidebar
// review). Line one holds the status mark, the session as sessionLabel
// names it with the port in quiet meta, and a right column with the eye
// and the count of the session's live snoops (Snoop SN5) before what
// its caller puts there. Line two says what the session is doing, with your health at
// its right. The caller's grid places each part, so the parts sit as
// siblings in it.

interface Props {
  row: SessionRow;
  rows: SessionRow[];
  mark: RowMark;
  /** What the right column of line one holds. */
  end: ReactNode;
  /** A double click on the name, which renames the session in the
   *  sidebar. */
  onNameDoubleClick?: () => void;
}

export function SessionRowBody({ row, rows, mark, end, onNameDoubleClick }: Props) {
  const label = sessionLabel(row, rows);
  const line = useSessionLine(row);
  const port = label.split?.port ?? label.meta;
  const snoops = useLiveSnoops(row.id);
  return (
    <>
      <SessionMark mark={mark} />
      <span className="shell-sessions-name" onDoubleClick={onNameDoubleClick}>
        <span className="shell-sessions-name-text">{label.split?.world ?? label.name}</span>
        {port && <span className="shell-sessions-port">{port}</span>}
      </span>
      <span className="shell-sessions-end">
        {snoops > 0 && <SnoopCount count={snoops} />}
        {end}
      </span>
      <SecondLine line={line} />
    </>
  );
}

/** The mark at the left of a row, in the words the title band uses. */
export function SessionMark({ mark }: { mark: RowMark }) {
  return (
    <span className={`shell-sessions-mark is-${mark}`} role="img" aria-label={MARK_WORDS[mark]}>
      {mark === 'hand' ? (
        <HandIcon />
      ) : mark === 'spinner' ? (
        <SpinnerIcon />
      ) : mark === 'triangle' ? (
        <TriangleIcon />
      ) : (
        <span className="shell-sessions-dot" />
      )}
    </span>
  );
}

/** How many things wait for you, up to 9+, white on the accent. */
export function WaitingCount({ count }: { count: number }) {
  return (
    <span className="shell-sessions-count" role="img" aria-label={`${count} waiting`}>
      {count > 9 ? '9+' : count}
    </span>
  );
}

/** How many snoops run in the session, the eye and the figure. */
function SnoopCount({ count }: { count: number }) {
  return (
    <span
      className="shell-sessions-snoops"
      role="img"
      aria-label={count === 1 ? '1 snoop' : `${count} snoops`}
    >
      <EyeIcon size={12} />
      <span>{count}</span>
    </span>
  );
}

/** A row's second line, with the health at its right when it shows. */
function SecondLine({ line }: { line: SessionLine }) {
  const { who, text, health, low } = line;
  return (
    <>
      <span className={health === null ? 'shell-sessions-line is-wide' : 'shell-sessions-line'}>
        {who && <span className="shell-sessions-who">{who}</span>}
        {who && text && ' · '}
        {text}
      </span>
      {health !== null && (
        <span className={low ? 'shell-sessions-health is-low' : 'shell-sessions-health'}>
          {health}%
        </span>
      )}
    </>
  );
}
