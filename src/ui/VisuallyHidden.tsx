import type { ReactNode } from 'react';

/** Text a screen reader reads and the page does not show, like a
 *  list row's On or Off. */
export function VisuallyHidden({ children, id }: { children: ReactNode; id?: string }) {
  return (
    <span id={id} className="visually-hidden">
      {children}
    </span>
  );
}
