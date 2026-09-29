import { useId, type ReactNode } from 'react';
import { Card } from './Card';
import { cx } from './cx';

export interface SectionProps {
  /** The h2, in sentence case. */
  title: ReactNode;
  /** Deep link and search anchor. The frame scrolls here when a target
   *  names it, so keep it in step with src/lib/settingsSearch.ts. */
  id?: string;
  /** Content at the right end of the heading row, like Appearance's
   *  import hint and button. */
  actions?: ReactNode;
  /** Wrap the children in a Card. True by default. Pass false to lay
   *  out your own cards or columns under the heading. */
  card?: boolean;
  className?: string;
  children?: ReactNode;
}

/** A settings section: a 28 px heading row with the h2 at 12/16 600 in
 *  the text color, then the card 8 px below it. Sections sit 16 px
 *  apart. */
export function Section({ title, id, actions, card = true, className, children }: SectionProps) {
  const headingId = useId();
  return (
    <section
      className={cx('st-section', className)}
      data-st-anchor={id}
      aria-labelledby={headingId}
    >
      <div className="st-section-head">
        <h2 id={headingId} className="st-section-title">
          {title}
        </h2>
        {actions !== undefined && <div className="st-section-actions">{actions}</div>}
      </div>
      {card ? <Card>{children}</Card> : children}
    </section>
  );
}
