import { useId, type ReactNode } from 'react';
import { openHelpTopic } from '../lib/helpLink';
import { Card } from './Card';
import { cx } from './cx';
import { IconButton } from './IconButton';
import { BookIcon } from './icons';

/** The help topic a section links to, and what the button names. */
export interface SectionHelp {
  /** A help topic id, like `shape.prompt-show`. */
  topic: string;
  /** What the topic is about, for the button's name, Help on <subject>. */
  subject: string;
}

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
  /** A book button at the end of the heading row, after any actions,
   *  that opens Help on this topic. */
  help?: SectionHelp;
  className?: string;
  children?: ReactNode;
}

/** A settings section: a 28 px heading row with the h2 at 12/16 600 in
 *  the text color, then the card 8 px below it. Sections sit 16 px
 *  apart. */
export function Section({
  title,
  id,
  actions,
  card = true,
  help,
  className,
  children,
}: SectionProps) {
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
        {(actions !== undefined || help !== undefined) && (
          <div className="st-section-actions">
            {actions}
            {help && (
              <IconButton
                label={`Help on ${help.subject}`}
                icon={<BookIcon />}
                onClick={() => openHelpTopic(help.topic)}
              />
            )}
          </div>
        )}
      </div>
      {card ? <Card>{children}</Card> : children}
    </section>
  );
}
