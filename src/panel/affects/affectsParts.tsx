import { affectMark, affectsSummary, type AffectRow } from './affectsView';
import { PaneHeader, PaneMeta } from '../PaneHeader';

// The pieces every Affects pane style draws the same way: the header
// that counts what you miss and what runs out, the sentence in place
// of the rows, the mark beside an affect, and the count of what waits
// on the next page.

/** The pane header: the caps label, then `N missing` and `N running
 *  out`. What does not fit is counted in the body, never here. */
export function AffectsHeader({ rows }: { rows: readonly AffectRow[] }) {
  const { missing, runningOut } = affectsSummary(rows);
  return (
    <PaneHeader
      meta={
        missing > 0 || runningOut > 0 ? (
          <>
            {missing > 0 && <PaneMeta tone="danger">{missing} missing</PaneMeta>}
            {runningOut > 0 && <PaneMeta tone="warn">{runningOut} running out</PaneMeta>}
          </>
        ) : null
      }
    />
  );
}

/** The sentence a pane shows in place of its rows. */
export function AffectsEmpty({ text }: { text: string }) {
  return <p className="pane-empty">{text}</p>;
}

/** The mark before an affect: a tracked affect's state, or the harmful
 *  diamond. The shape comes from the body's data-affects-marker. */
export function AffectMark({ row }: { row: AffectRow }) {
  const mark = affectMark(row);
  return mark ? <span className={`pane-affect-mark is-${mark}`} aria-hidden="true" /> : null;
}

/** The count of what waits on the next page. A click scrolls to it. */
export function MoreButton({ count, onClick }: { count: number; onClick: () => void }) {
  return (
    <button
      type="button"
      className="pane-affects-more"
      aria-label={`${count} more affects, scroll to them`}
      onClick={onClick}
    >
      {count} more
    </button>
  );
}
