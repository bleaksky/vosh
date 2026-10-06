import { useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { CARD_FOOT, type CardFacts } from './cardFacts';

// The card that opens beside a session's row when you point at it, S6
// of the Sessions Sidebar review, boards 03 and 07. It holds what does
// not fit on the row's two lines: the world with the profile, then the
// room, the area, the fight, your vitals, how long you have been online
// and what waits for you, each row dropping out while the session has
// nothing for it. A session that is not connected shows the world, the
// profile and when it last played. The foot says how to rename.
//
// It floats on the recipe of the title band's menus, 12 right of the
// sidebar line and level with the row. useHoverCard says when it opens
// and closes, and cardFacts what it says. It never takes focus. A
// screen reader hears the same text as the row's description, so the
// card itself is hidden from it.

/** How far right of the sidebar line the card sits. */
const GAP = 12;
/** How far inside the window it stays. */
const INSET = 8;

interface CardProps {
  facts: CardFacts;
  /** The row's slot, which the card sits level with. */
  slot: HTMLElement;
  /** The sidebar, whose line the card sits right of. */
  side: HTMLElement;
}

/** The card itself, in the body over everything, as the menus are. */
export function SessionCard({ facts, slot, side }: CardProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  // Placed before the first paint, and moved up when it would run past
  // the foot of the window.
  useLayoutEffect(() => {
    const height = ref.current?.offsetHeight ?? 0;
    const top = slot.getBoundingClientRect().top;
    setPos({
      left: Math.round(side.getBoundingClientRect().right + GAP),
      top: Math.round(Math.max(INSET, Math.min(top, window.innerHeight - INSET - height))),
    });
  }, [facts, slot, side]);

  return createPortal(
    <div
      ref={ref}
      className="shell-sessions-card"
      aria-hidden="true"
      style={pos ?? { visibility: 'hidden' }}
    >
      <div className="shell-sessions-card-head">
        <span className="shell-sessions-card-name">{facts.name}</span>
        {facts.port && <span className="shell-sessions-port">{facts.port}</span>}
      </div>
      {facts.where && <p className="shell-sessions-card-where">{facts.where}</p>}
      {facts.facts.length > 0 && (
        <dl className="shell-sessions-card-facts">
          {facts.facts.map((f) => (
            <div key={f.label} className="shell-sessions-card-fact">
              <dt>{f.label}</dt>
              <dd className={f.low ? 'is-low' : undefined}>{f.value}</dd>
            </div>
          ))}
        </dl>
      )}
      <p className="shell-sessions-card-foot">{CARD_FOOT}</p>
    </div>,
    document.body,
  );
}
