import type { ReactNode } from 'react';

/** A quiet line of copy inside a card, above its rows. `tone="warn"`
 *  sets it in the warn color after the pane status dot, for what needs
 *  you about the card's item, like a plugin Vosh stopped. */
export function CardNote({ tone, children }: { tone?: 'warn'; children: ReactNode }) {
  if (tone === 'warn') {
    return (
      <p className="st-card-note is-warn">
        <span className="st-warn-dot" aria-hidden="true" />
        <span>{children}</span>
      </p>
    );
  }
  return <p className="st-card-note">{children}</p>;
}
