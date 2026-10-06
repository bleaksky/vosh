import type { MouseEvent } from 'react';
import type { SessionRow } from '../ipc/session';
import { sessionLabel } from '../lib/sessionLabel';
import { PlusIcon } from '../ui/icons';
import { SidebarIcon } from './icons';

// The sessions sidebar on the left of the main window, board 2 of the
// Sessions review, drawn to otty's measures (Q17). MainWindow shows it
// while two or more sessions are open and you have not hidden it in this
// window. Its top 32 drags the window and holds the lights on macOS, with
// New session and Hide sessions at its right. SESSIONS heads the list,
// and each row reads the session as sessionLabel names it, the selected
// one a filled pill. A click selects.
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

export function SessionSidebar({ rows, selected, onSelect, onNewSession, onHide, onCaret }: Props) {
  const held = (e: MouseEvent<HTMLButtonElement>) => document.activeElement === e.currentTarget;
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
        {rows.map((row) => {
          const label = sessionLabel(row, rows);
          return (
            <li key={row.id} className="shell-sessions-slot">
              <button
                type="button"
                className="shell-sessions-row"
                aria-current={row.id === selected ? 'true' : undefined}
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
                {label.meta && <span className="shell-sessions-meta">{label.meta}</span>}
              </button>
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
