import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from 'react';
import type { KindNoun } from '../../automation/automationDraft';
import {
  foldKeyOf,
  groupOfSectionKey,
  listKeyAction,
  listStops,
  sectionKeyOf,
  stopId,
  tabStopId,
  type ListEntry,
  type ListSection,
  type ListStop,
} from '../../automation/automationList';
import { loadoutHoldNote } from '../../automation/groupSwitches';
import { scrollWithin } from '../../lib/scrollWithin';
import { ChevronRightIcon, cx, Field, SearchIcon, Toggle, VisuallyHidden } from '../../ui';
import type { GroupSwitches } from './useGroupSwitches';

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
  /** How the dot draws while the row is off, see ListEntry. */
  dot: ListEntry['dot'];
  /** Why the row carries the warn ring, which a reader hears as its
   *  description. Undefined for a row with no ring. */
  warnNote: string | undefined;
  onSelect: (uid: string) => void;
  onFocus: () => void;
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
  dot,
  warnNote,
  onSelect,
  onFocus,
}: RowProps) {
  const warn = warnNote !== undefined;
  const suggested = !enabled && dot === 'suggested';
  const noteId = warn ? `st-auto-warn-${uid}` : undefined;
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
        onFocus={onFocus}
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
        <span
          className={cx('st-auto-dot', !enabled && 'is-off', suggested && 'is-suggested')}
          aria-hidden="true"
        />
        <VisuallyHidden>{enabled ? 'On' : suggested ? 'Suggested, off' : 'Off'}</VisuallyHidden>
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
  /** The rows that carry the warn ring while they are on, by name, each
   *  with why, like a trigger that hides your prompt with nothing drawn
   *  in its place. A reader hears the why as the row's description,
   *  since the ring is a picture. */
  warnNotes?: ReadonlyMap<string, string> | undefined;
  /** The groups that show folded, by fold key. */
  folded: ReadonlySet<string>;
  /** Fold or open a group from its heading. */
  onFold: (key: string, fold: boolean) => void;
  /** The on and off switch after each group heading. Null or left out
   *  for lists whose groups have none. */
  groupSwitches?: GroupSwitches | null | undefined;
}

/** A heading you moved to, and the selection it was made under. */
interface HeadingCursor {
  id: string;
  selected: string | null;
}

/** The Automation list: the filter field, then the rows under their
 *  group headings, with the divider block between groups. Each heading
 *  folds its group away and opens it again. Up and Down move through
 *  the headings and rows as one list, a row taking the selection as
 *  you reach it, and Left and Right fold and open a heading. One stop
 *  takes Tab, the selected row unless you moved to a heading. A group
 *  heading can carry the switch that turns its whole group on and off.
 *  Tab reaches it from its heading while that heading holds the stop,
 *  so the list still takes one Tab from outside. */
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
  warnNotes,
  folded,
  onFold,
  groupSwitches,
}: ItemListProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const baseId = useId();
  const [cursor, setCursor] = useState<HeadingCursor | null>(null);
  const stops: ListStop[] = listStops(sections, folded);
  if (pinned) stops.unshift({ kind: 'row', uid: pinned.uid });
  const heading = cursor !== null && cursor.selected === selected ? cursor.id : null;
  const tabId = tabStopId(stops, sections, folded, selected, heading);
  const placeholder = `Untitled ${noun.one}`;
  const onRowFocus = useCallback(() => setCursor(null), []);

  useEffect(() => {
    if (revealSeq === 0 || selected === null) return;
    const row = scrollRef.current?.querySelector<HTMLElement>(
      `[data-uid="${CSS.escape(selected)}"]`,
    );
    scrollWithin(row, { block: 'nearest' });
    // Only a new reveal request moves the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealSeq]);

  /** The element for a stop, found by its data attribute. */
  const stopElement = (stop: ListStop): HTMLElement | null => {
    const selector =
      stop.kind === 'row'
        ? `[data-uid="${CSS.escape(stop.uid)}"]`
        : `[data-fold="${CSS.escape(stop.key)}"]`;
    return scrollRef.current?.querySelector<HTMLElement>(selector) ?? null;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    // The stop with focus, else the one that takes Tab, as when focus
    // sits on Edit all as JSON… under the list.
    // A group switch moves as its heading does.
    const { uid, fold, groupSwitch } = (e.target as HTMLElement).dataset ?? {};
    const at =
      uid !== undefined
        ? stopId({ kind: 'row', uid })
        : fold !== undefined
          ? stopId({ kind: 'heading', key: fold })
          : groupSwitch !== undefined
            ? stopId({ kind: 'heading', key: sectionKeyOf({ group: groupSwitch }) })
            : tabId;
    const action = listKeyAction(stops, at, e.key);
    // Left and Right fold only the heading that has focus.
    if (!action || (action.type === 'fold' && fold === undefined)) return;
    e.preventDefault();
    if (action.type === 'fold') {
      if (folded.has(action.key) !== action.fold) onFold(action.key, action.fold);
      return;
    }
    const stop = action.to;
    if (stop.kind === 'row') onSelect(stop.uid);
    const el = stopElement(stop);
    // Focus would scroll the stop into view on its own, centered when it
    // was hidden and in every box above it. Keep that off so the list
    // steps one stop at a time and nothing else moves.
    el?.focus({ preventScroll: true });
    scrollWithin(el, { block: 'nearest' });
  };

  const onHeadingClick = (e: MouseEvent<HTMLButtonElement>, key: string, open: boolean) => {
    // Keep focus on the heading, even where a click focuses no button,
    // so focus never sits on a row the click folds away.
    e.currentTarget.focus({ preventScroll: true });
    onFold(key, open);
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
              tabbable={tabId === stopId({ kind: 'row', uid: pinned.uid })}
              placeholder={placeholder}
              monoName={false}
              monoMeta={false}
              anchor={pinned.anchor}
              dot={undefined}
              warnNote={undefined}
              onSelect={onSelect}
              onFocus={onRowFocus}
            />
          )}
          {sections.map((section, index) => {
            const key = foldKeyOf(section);
            const open = key === null || !folded.has(key);
            const rows = open
              ? section.entries.map((entry) => (
                  <ListRow
                    key={entry.uid}
                    uid={entry.uid}
                    name={entry.name}
                    meta={entry.meta}
                    enabled={entry.enabled}
                    selected={selected === entry.uid}
                    tabbable={tabId === stopId({ kind: 'row', uid: entry.uid })}
                    placeholder={placeholder}
                    monoName={monoName}
                    monoMeta={monoMeta}
                    anchor={entry.anchor}
                    dot={entry.dot}
                    warnNote={entry.enabled ? warnNotes?.get(entry.name) : undefined}
                    onSelect={onSelect}
                    onFocus={onRowFocus}
                  />
                ))
              : null;
            const divider = (index > 0 || pinned) && (
              <div className="st-auto-divider" aria-hidden="true" />
            );
            // The ungrouped items at the top have no heading and never fold.
            if (key === null) {
              return (
                <Fragment key={section.key}>
                  {divider}
                  {rows}
                </Fragment>
              );
            }
            const id = stopId({ kind: 'heading', key });
            const groupId = `${baseId}-group-${index}`;
            const count = section.entries.length;
            const group = groupOfSectionKey(section.key);
            const groupSwitch = group !== null ? groupSwitches?.byName.get(group) : undefined;
            const hold = groupSwitch?.loadouts;
            const noteId = hold ? `${groupId}-note` : undefined;
            return (
              <Fragment key={section.key}>
                {divider}
                <div className={cx('st-auto-headrow', index === 0 && !pinned && 'is-first')}>
                  <h2 className="st-auto-heading">
                    <button
                      type="button"
                      className="st-auto-fold"
                      aria-expanded={open}
                      aria-controls={open ? groupId : undefined}
                      aria-describedby={noteId}
                      tabIndex={tabId === id ? 0 : -1}
                      data-fold={key}
                      onClick={(e) => onHeadingClick(e, key, open)}
                      onFocus={() => setCursor({ id, selected })}
                    >
                      <ChevronRightIcon size={12} className="st-auto-fold-chevron" />
                      <span className="st-auto-fold-name">{section.heading}</span>
                      {!open && (
                        <span className="st-auto-fold-count">
                          {count}
                          <VisuallyHidden> {count === 1 ? noun.one : noun.many}</VisuallyHidden>
                        </span>
                      )}
                    </button>
                  </h2>
                  {/* The group's on and off switch sits after the heading,
                      since a switch cannot sit inside a button. */}
                  {group !== null && groupSwitch && groupSwitches && (
                    <Toggle
                      className="st-auto-groupswitch"
                      aria-label={`${group} group`}
                      aria-describedby={noteId}
                      checked={groupSwitch.enabled}
                      disabled={hold !== undefined}
                      tabIndex={tabId === id ? 0 : -1}
                      data-group-switch={group}
                      onFocus={() => setCursor({ id, selected })}
                      onChange={(on) => groupSwitches.turn(group, on)}
                    />
                  )}
                </div>
                {hold && groupSwitch && (
                  <p id={noteId} className="st-auto-groupnote">
                    {loadoutHoldNote(hold, groupSwitch.enabled)}
                  </p>
                )}
                {open && (
                  <div id={groupId} className="st-auto-group">
                    {rows}
                  </div>
                )}
              </Fragment>
            );
          })}
        </div>
        {empty && <p className="st-auto-empty">{empty}</p>}
        {footer && <div className="st-auto-footer">{footer}</div>}
      </div>
    </div>
  );
}
