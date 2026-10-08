import { forwardRef, Fragment, type CSSProperties, type ReactNode } from 'react';
import { parseHelpBody, type HelpAction, type HelpTopic } from './helpContent';
import { inlinePieces, keyGlyph, keyParts, type InlinePiece } from './helpInline';
import { helpItemId, matchRanges, type OutlineEntry } from './helpNav';
import { openGetStarted } from '../ipc/getStarted';
import { Button, Keycap } from '../ui';

// One help topic as the approved Help boards draw it: the H1 at 26/32,
// prose and lists on a 528 measure at 14/22, a table as a Settings
// card, a button as a primary Settings button, and each backticked
// span as a mono chip, an SF 600 label, or keycaps
// (src/help/helpInline.ts). While the search holds words every
// match is marked the way the session logs page marks one, and the
// match you are on carries a ring.

interface Props {
  topic: HelpTopic;
  /** The words to mark, or empty for none. */
  query: string;
  /** The match you are on, counted from 0 in reading order. */
  current: number;
  /** The rows of On this page, whose items take ids to scroll to. */
  outline: OutlineEntry[] | null;
  /** The mark fill and its ring, from the theme's ANSI yellow. */
  markColors: { fill: string; ring: string } | null;
}

/** What each help button runs. */
const RUN_ACTION: Record<HelpAction, () => void> = {
  'get-started': () => {
    openGetStarted().catch((e: unknown) => console.error('[help] open get started failed', e));
  },
};

/** Hands out match numbers in reading order as the article draws. */
class Marker {
  next = 0;
  constructor(
    readonly query: string,
    readonly current: number,
  ) {}

  /** `text` with each match wrapped in a mark. */
  text(text: string): ReactNode {
    const ranges = matchRanges(text, this.query);
    if (ranges.length === 0) return text;
    const out: ReactNode[] = [];
    let at = 0;
    for (const [start, end] of ranges) {
      if (start > at) out.push(text.slice(at, start));
      const n = this.next++;
      out.push(
        <mark
          key={start}
          className="hp-mark"
          data-match={n}
          data-current={n === this.current ? '' : undefined}
        >
          {text.slice(start, end)}
        </mark>,
      );
      at = end;
    }
    if (at < text.length) out.push(text.slice(at));
    return out;
  }

  /** Mark a whole run, like a row of keycaps, once for each match. */
  whole(text: string, node: ReactNode): ReactNode {
    const count = matchRanges(text, this.query).length;
    if (count === 0) return node;
    const first = this.next;
    this.next += count;
    const current = this.current >= first && this.current < this.next;
    return (
      <mark className="hp-mark" data-match={first} data-current={current ? '' : undefined}>
        {node}
      </mark>
    );
  }
}

function piece(p: InlinePiece, key: number, marker: Marker): ReactNode {
  switch (p.kind) {
    case 'text':
      return <Fragment key={key}>{marker.text(p.text)}</Fragment>;
    case 'label':
      return (
        <strong key={key} className="hp-label">
          {marker.text(p.text)}
        </strong>
      );
    case 'code':
      return (
        <code key={key} className="hp-chip">
          {marker.text(p.text)}
        </code>
      );
    case 'key': {
      const keys = (
        <kbd className="hp-keys">
          {(keyParts(p.text) ?? [p.text]).map((part, i) => (
            <Keycap key={i}>{keyGlyph(part)}</Keycap>
          ))}
        </kbd>
      );
      return <Fragment key={key}>{marker.whole(p.text, keys)}</Fragment>;
    }
  }
}

function line(text: string, marker: Marker): ReactNode[] {
  return inlinePieces(text).map((p, i) => piece(p, i, marker));
}

/** A cell that holds only codes, like `%hp` `%mana` `%move`, draws them
 *  as a row of mono codes 12 apart, the code column of the card. */
function codeCell(text: string, marker: Marker): ReactNode | null {
  const pieces = inlinePieces(text);
  const codes = pieces.filter((p) => p.kind === 'code');
  if (codes.length === 0 || pieces.some((p) => p.kind !== 'code' && p.text.trim() !== '')) {
    return null;
  }
  return (
    <span className="hp-codes">
      {codes.map((p, i) => (
        <code key={i}>{marker.text(p.text)}</code>
      ))}
    </span>
  );
}

export const HelpArticle = forwardRef<HTMLHeadingElement, Props>(function HelpArticle(
  { topic, query, current, outline, markColors },
  titleRef,
) {
  const marker = new Marker(query, current);
  const outlined = new Set((outline ?? []).map((e) => helpItemId(e.block, e.item)));
  const style = markColors
    ? ({ '--hp-mark': markColors.fill, '--hp-mark-ring': markColors.ring } as CSSProperties)
    : undefined;
  // The title first, then each block in order, so the match numbers run
  // in reading order, the order countMatches counts them in.
  const title = marker.text(topic.title);
  const blocks = parseHelpBody(topic.body).map((block, b) => {
    if (block.kind === 'paragraph') {
      return <p key={b}>{line(block.text, marker)}</p>;
    }
    if (block.kind === 'action') {
      return (
        <div key={b} className="hp-actions">
          <Button variant="primary" onClick={RUN_ACTION[block.action]}>
            {marker.text(block.label)}
          </Button>
        </div>
      );
    }
    if (block.kind === 'list') {
      return (
        <ul key={b}>
          {block.items.map((item, i) => {
            const id = helpItemId(b, i);
            return (
              <li key={i} id={outlined.has(id) ? id : undefined}>
                {line(item, marker)}
              </li>
            );
          })}
        </ul>
      );
    }
    const head = block.head.map((cell, c) => (
      <th key={c} scope="col">
        {line(cell, marker)}
      </th>
    ));
    const rows = block.rows.map((row, r) => (
      <tr key={r}>
        {row.map((cell, c) => (
          <td key={c}>{codeCell(cell, marker) ?? line(cell, marker)}</td>
        ))}
      </tr>
    ));
    return (
      <table key={b} className="hp-table">
        <thead>
          <tr>{head}</tr>
        </thead>
        <tbody>{rows}</tbody>
      </table>
    );
  });
  return (
    <article className="hp-article" aria-labelledby="hp-title" style={style}>
      <h1 id="hp-title" ref={titleRef} className="hp-title">
        {title}
      </h1>
      {blocks}
    </article>
  );
});
