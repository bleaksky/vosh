import { useRef, useState, type MouseEvent } from 'react';
import type { ModePill as Pill } from './modePill';
import { ChevronDownIcon, VisuallyHidden } from '../ui';
import { MenuItem, MenuSurface, type MenuCloseReason, type MenuPlacer } from '../ui/MenuSurface';

// The pill at the start of the command line: the mode's name, then its
// count after a dot, the count in warn past the editor's limit. A screen
// reader hears the label, which reads the count as words. In the game's
// editor the pill is a button that opens a menu over it, with Open in
// the writing card and Finish.

/** What the editor pill's menu does. */
export interface EditorActions {
  /** Open the writing card on the text the game's editor holds, or null
   *  for a text the card does not take. */
  onOpenWriting: (() => void) | null;
  /** Send @, which ends the game's editor. */
  onFinish: () => void;
  /** Put the caret back in the command line. */
  onReturn: () => void;
}

// A press on the pill leaves the caret on the command line.
const keepCaret = (event: MouseEvent) => event.preventDefault();

// The space between the pill and the menu over it.
const GAP = 6;
// Space kept between a menu and the window edge.
const EDGE = 8;

/** A menu over `anchor`, from its left edge, kept inside the window. */
function menuOver(anchor: HTMLElement): MenuPlacer {
  return (size, viewport) => {
    const r = anchor.getBoundingClientRect();
    const left = Math.max(EDGE, Math.min(r.left, viewport.width - size.width - EDGE));
    return { left: Math.round(left), top: Math.round(Math.max(EDGE, r.top - GAP - size.height)) };
  };
}

export function LinePill({ pill, editor }: { pill: Pill; editor: EditorActions | null }) {
  if (pill.mode === 'editor' && editor) return <EditorPill pill={pill} actions={editor} />;
  return (
    <span className="input-pill" role="status">
      <VisuallyHidden>{pill.label}</VisuallyHidden>
      <PillFace pill={pill} />
    </span>
  );
}

function EditorPill({ pill, actions }: { pill: Pill; actions: EditorActions }) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const close = (reason: MenuCloseReason | 'select') => {
    setOpen(false);
    if (reason !== 'outside') actions.onReturn();
  };
  const run = (action: () => void) => () => {
    close('select');
    action();
  };
  return (
    <>
      <button
        ref={ref}
        type="button"
        className="input-pill"
        aria-label={pill.label}
        aria-haspopup="menu"
        aria-expanded={open}
        onMouseDown={keepCaret}
        onClick={() => setOpen((v) => !v)}
      >
        <PillFace pill={pill} />
        <ChevronDownIcon size={12} />
      </button>
      {open && ref.current && (
        <MenuSurface
          label={pill.name}
          anchor={ref.current}
          at={menuOver(ref.current)}
          onClose={close}
        >
          {actions.onOpenWriting && (
            <MenuItem onSelect={run(actions.onOpenWriting)}>Open in the writing card</MenuItem>
          )}
          <MenuItem
            onSelect={run(actions.onFinish)}
            trailing={
              <kbd className="menu-keys" aria-hidden="true">
                @
              </kbd>
            }
          >
            Finish
          </MenuItem>
        </MenuSurface>
      )}
    </>
  );
}

function PillFace({ pill }: { pill: Pill }) {
  return (
    <>
      <span className="input-pill-name" aria-hidden="true">
        {pill.name}
      </span>
      {pill.count !== null && (
        <>
          <span className="input-pill-dot" aria-hidden="true">
            ·
          </span>
          <span className={`input-pill-count${pill.warn ? ' is-warn' : ''}`} aria-hidden="true">
            {pill.count}
          </span>
        </>
      )}
    </>
  );
}
