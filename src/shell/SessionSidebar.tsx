import type { ComponentType, MouseEvent } from 'react';
import type { SessionRow } from '../ipc/session';
import { sessionLabel } from '../lib/sessionLabel';
import { rowLook, useSessionRow, type RowGlyph } from '../stores/session/sessionRowStore';
import { DotIcon, HandIcon, PlusIcon, SpinnerIcon, TriangleIcon } from '../ui/icons';
import { SidebarIcon } from './icons';

// The sessions sidebar on the left of the main window, board 2 of the
// Sessions review, drawn to otty's measures (Q17). MainWindow shows it
// while two or more sessions are open and you have not hidden it in this
// window. Its top 32 drags the window and holds the lights on macOS, with
// New session and Hide sessions at its right. SESSIONS heads the list,
// and each row reads the session as sessionLabel names it, the selected
// one a filled pill. A row's glyph takes the meta's place while it shows,
// and its name takes the tone the row store gives it (board 3). A click
// selects.
//
// WebView2 and WebKitGTK focus a button on click. Left on a row or Hide
// sessions, the caret would take your next Space and press it again, so
// it goes back to the command line, as it does from the gear.

interface Props {
  rows: SessionRow[];
  selected: number;
  onSelect: (session: number) => void;
  onNewSession: () => void;
  /** Fold the sidebar away in this window. */
  onHide: () => void;
  /** Hand the caret back to the command line. */
  onCaret: () => void;
}

/** Whether a click left the caret on the button it pressed. */
const held = (e: MouseEvent<HTMLButtonElement>) => document.activeElement === e.currentTarget;

export function SessionSidebar({ rows, selected, onSelect, onNewSession, onHide, onCaret }: Props) {
  return (
    <aside className="shell-sessions st-controls" aria-label="Sessions">
      <div className="shell-sessions-top" data-tauri-drag-region>
        <div className="shell-sessions-actions">
          <button
            type="button"
            className="shell-icon-button"
            aria-label="New session"
            onClick={onNewSession}
          >
            <PlusIcon />
          </button>
          <button
            type="button"
            className="shell-icon-button"
            aria-label="Hide sessions"
            onClick={(e) => {
              const caret = held(e);
              onHide();
              if (caret) onCaret();
            }}
          >
            <SidebarIcon />
          </button>
        </div>
      </div>
      <h2 className="shell-sessions-head">Sessions</h2>
      <ul className="shell-sessions-list">
        {rows.map((row) => (
          <SessionSlot
            key={row.id}
            row={row}
            rows={rows}
            current={row.id === selected}
            onSelect={onSelect}
            onCaret={onCaret}
          />
        ))}
      </ul>
    </aside>
  );
}

/** Each glyph, with the words a screen reader says for it, from board
 *  3 and Q8, where the triangle means connect again yourself. */
const GLYPHS: Record<RowGlyph, { icon: ComponentType; words: string }> = {
  triangle: { icon: TriangleIcon, words: 'Connect again' },
  hand: { icon: HandIcon, words: 'Logging in' },
  spinner: { icon: SpinnerIcon, words: 'Connecting' },
  dot: { icon: DotIcon, words: 'Something for you' },
};

interface SlotProps {
  row: SessionRow;
  rows: SessionRow[];
  current: boolean;
  onSelect: (session: number) => void;
  onCaret: () => void;
}

/** One session's row. */
function SessionSlot({ row, rows, current, onSelect, onCaret }: SlotProps) {
  const label = sessionLabel(row, rows);
  const { glyph, tone } = rowLook(useSessionRow(row.id), row, current);
  return (
    <li className="shell-sessions-slot">
      <button
        type="button"
        className={tone ? `shell-sessions-row is-${tone}` : 'shell-sessions-row'}
        aria-current={current ? 'true' : undefined}
        title={label.tooltip ?? undefined}
        onClick={(e) => {
          onSelect(row.id);
          if (held(e)) onCaret();
        }}
      >
        {label.split ? (
          <span className="shell-sessions-name is-world">
            <span className="shell-sessions-world">{label.split.world}</span>
            {label.split.port}
          </span>
        ) : (
          <span className="shell-sessions-name">{label.name}</span>
        )}
        {glyph ? (
          <RowGlyphMark glyph={glyph} />
        ) : (
          label.meta && <span className="shell-sessions-meta">{label.meta}</span>
        )}
      </button>
    </li>
  );
}

function RowGlyphMark({ glyph }: { glyph: RowGlyph }) {
  const { icon: Icon, words } = GLYPHS[glyph];
  return (
    <span className={`shell-sessions-glyph is-${glyph}`} role="img" aria-label={words}>
      <Icon />
    </span>
  );
}
