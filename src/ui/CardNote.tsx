import type { ReactNode } from 'react';

/** A quiet line of copy inside a card, above its rows. `tone="warn"`
 *  sets it in the warn color after the pane status dot, for what needs
 *  you about the card's item, like a plugin Vosh stopped. `action`, a
 *  button, sits at the end of a warn note, for the way to fix it. */
export function CardNote({
  tone,
  action,
  children,
}: {
  tone?: 'warn';
  action?: ReactNode;
  children: ReactNode;
}) {
  if (tone === 'warn') {
    return (
      <p className="st-card-note is-warn">
        <span className="st-warn-dot dot is-warn" aria-hidden="true" />
        <span>{children}</span>
        {action}
      </p>
    );
  }
  return <p className="st-card-note">{children}</p>;
}
