import { Fragment, memo, useEffect, useRef, type KeyboardEvent, type ReactNode } from 'react';
import type { KindNoun } from '../../../../lib/automationDraft';
import type { ListSection } from '../../../../lib/automationList';
import { cx, Field, SearchIcon, VisuallyHidden } from '../../ui';

/** A row pinned above the groups, like the Tick in Timers. */
export interface PinnedEntry {
  uid: string;
  name: string;
  enabled: boolean;
  /** Deep link and search anchor, like `tick`. */
  anchor: string;
}

interface RowProps {
  uid: string;
  name: string;
  meta: string | undefined;
  enabled: boolean;
  selected: boolean;
  tabbable: boolean;
  placeholder: string;
  monoName: boolean;
  monoMeta: boolean;
  anchor: string | undefined;
  /** Carries the warn ring. */
  warn: boolean;
  /** Why it carries it, which a reader hears as the row's description. */
  warnNote: string | undefined;
  onSelect: (uid: string) => void;
}

// One list row: 36 high on a 38 pitch, the name ellipsized, then the
// 8 px dot. Memoized, so an edit to one item redraws one row even in
// a list of 500.
const ListRow = memo(function ListRow({
  uid,
  name,
  meta,
  enabled,
  selected,
  tabbable,
  placeholder,
  monoName,
  monoMeta,
  anchor,
  warn,
  warnNote,
  onSelect,
}: RowProps) {
  const noteId = warn && warnNote ? `st-auto-warn-${uid}` : undefined;
  return (
    <div className="st-auto-rowwrap">
      <button
        type="button"
        className={cx('st-auto-row', warn && 'is-warn')}
        aria-current={selected ? 'true' : undefined}
        aria-describedby={noteId}
        tabIndex={tabbable ? 0 : -1}
        data-uid={uid}
        data-st-anchor={anchor}
        data-st-flash={anchor ? '' : undefined}
        onClick={() => onSelect(uid)}
      >
        <span
          className={cx(
            'st-auto-row-name',
            monoName && name && 'st-auto-mono',
            !name && 'st-auto-row-untitled',
          )}
        >
          {name || placeholder}
        </span>
        {meta && <span className={cx('st-auto-row-meta', monoMeta && 'st-auto-mono')}>{meta}</span>}
        <span className={cx('st-auto-dot', !enabled && 'is-off')} aria-hidden="true" />
        <VisuallyHidden>{enabled ? 'On' : 'Off'}</VisuallyHidden>
        {noteId && (
          <span id={noteId} hidden>
            {warnNote}
          </span>
        )}
      </button>
    </div>
  );
});

export interface ItemListProps {
  noun: KindNoun;
  filterLabel: string;
  filter: string;
  onFilter: (value: string) => void;
  /** The sections after the filter. */
  sections: readonly ListSection[];
  /** Whether the kind has any items before the filter. */
  hasItems: boolean;
  emptyText: string;
  pinned: PinnedEntry | null;
  selected: string | null;
  onSelect: (uid: string) => void;
  /** Bring the selected row into view each time this changes. */
  revealSeq: number;
  monoName: boolean;
  monoMeta: boolean;
  /** Quiet content under the list, like Edit all as JSON…. */
  footer?: ReactNode;
  /** Names of rows that carry the warn ring while they are on, like a
   *  trigger that hides your prompt with nothing drawn in its place. */
  warnNames?: ReadonlySet<string> | undefined;
  /** Why a row carries the warn ring, which a reader hears as its
   *  description, since the ring is a picture. */
  warnNote?: string | undefined;
}

/** The Automation list: the filter field, then the rows under their
 *  group headings, with the divider block between groups. Arrow keys
 *  move the selection, and only the selected row takes Tab. */
export function ItemList({
  noun,
  filterLabel,
  filter,
  onFilter,
  sections,
  hasItems,
  emptyText,
  pinned,
  selected,
  onSelect,
  revealSeq,
  monoName,
  monoMeta,
  footer,
  warnNames,
  warnNote,
}: ItemListProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const order: string[] = [];
  if (pinned) order.push(pinned.uid);
  for (const section of sections) for (const entry of section.entries) order.push(entry.uid);
  const tabUid = selected !== null && order.includes(selected) ? selected : (order[0] ?? null);
  const placeholder = `Untitled ${noun.one}`;

  useEffect(() => {
    if (revealSeq === 0 || selected === null) return;
    const row = scrollRef.current?.querySelector<HTMLElement>(
      `[data-uid="${CSS.escape(selected)}"]`,
    );
    row?.scrollIntoView({ block: 'nearest' });
    // Only a new reveal request moves the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealSeq]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (order.length === 0) return;
    const at = tabUid === null ? -1 : order.indexOf(tabUid);
    let next = at;
    if (e.key === 'ArrowDown') next = Math.min(order.length - 1, at + 1);
    else if (e.key === 'ArrowUp') next = Math.max(0, at - 1);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = order.length - 1;
    else return;
    e.preventDefault();
    const uid = order[next];
    onSelect(uid);
    const row = scrollRef.current?.querySelector<HTMLElement>(`[data-uid="${CSS.escape(uid)}"]`);
    row?.focus();
    row?.scrollIntoView({ block: 'nearest' });
  };

  let empty: string | null = null;
  if (!hasItems && !pinned) empty = emptyText;
  else if (hasItems && sections.length === 0 && filter.trim()) empty = `No ${noun.many} match.`;

  return (
    <div className="st-auto-list">
      <Field
        icon={<SearchIcon />}
        type="search"
        width={240}
        className="st-auto-filter"
        value={filter}
        onChange={onFilter}
        placeholder={filterLabel}
        aria-label={filterLabel}
      />
      <div ref={scrollRef} className="st-auto-scroll" onKeyDown={onKeyDown}>
        <div className="st-auto-sections">
          {pinned && (
            <ListRow
              uid={pinned.uid}
              name={pinned.name}
              meta={undefined}
              enabled={pinned.enabled}
              selected={selected === pinned.uid}
              tabbable={tabUid === pinned.uid}
              placeholder={placeholder}
              monoName={false}
              monoMeta={false}
              anchor={pinned.anchor}
              warn={false}
              warnNote={undefined}
              onSelect={onSelect}
            />
          )}
          {sections.map((section, index) => (
            <Fragment key={section.key}>
              {(index > 0 || pinned) && <div className="st-auto-divider" aria-hidden="true" />}
              {section.heading !== null && (
                <h2 className={cx('st-auto-heading', index === 0 && !pinned && 'is-first')}>
                  {section.heading}
                </h2>
              )}
              {section.entries.map((entry) => (
                <ListRow
                  key={entry.uid}
                  uid={entry.uid}
                  name={entry.name}
                  meta={entry.meta}
                  enabled={entry.enabled}
                  selected={selected === entry.uid}
                  tabbable={tabUid === entry.uid}
                  placeholder={placeholder}
                  monoName={monoName}
                  monoMeta={monoMeta}
                  anchor={undefined}
                  warn={entry.enabled && (warnNames?.has(entry.name) ?? false)}
                  warnNote={warnNote}
                  onSelect={onSelect}
                />
              ))}
            </Fragment>
          ))}
        </div>
        {empty && <p className="st-auto-empty">{empty}</p>}
        {footer && <div className="st-auto-footer">{footer}</div>}
      </div>
    </div>
  );
}
