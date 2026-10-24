import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useId,
  useMemo,
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
import { loadoutHoldNote, newGroupNote } from '../../automation/groupSwitches';
import { pickState } from '../../automation/listPick';
import { scrollWithin } from '../../lib/scrollWithin';
import { isMacPlatform } from '../../lib/shortcuts';
import {
  CheckboxIcon,
  CheckIcon,
  ChevronRightIcon,
  cx,
  Field,
  IconButton,
  MoreIcon,
  PencilIcon,
  SearchIcon,
  Toggle,
  VisuallyHidden,
} from '../../ui';
import type { MenuPlacement } from '../../ui/MenuSurface';
import type { GroupSwitches } from './useGroupSwitches';

/** A row pinned above the groups, like the Tick in Timers. */
export interface PinnedEntry {
  uid: string;
  name: string;
  enabled: boolean;
  /** Deep link and search anchor, like `tick`. */
  anchor: string;
}

/** Picking several rows, for a list whose kind can change many at
 *  once. listPick.ts holds the rules. */
export interface ListPicking {
  /** Picking is on: each row wears its checkbox and a click checks it. */
  on: boolean;
  checked: ReadonlySet<string>;
  /** Check or clear a row, or check the run to it. */
  row: (uid: string, how: 'toggle' | 'range') => void;
  /** Check or clear every row of a group, from its heading. */
  group: (uids: readonly string[], check: boolean) => void;
  /** Check every row you can see. */
  all: () => void;
  start: () => void;
  end: () => void;
}

interface RowProps {
  uid: string;
  name: string;
  meta: string | undefined;
  enabled: boolean;
  selected: boolean;
  /** The row's checkbox while picking is on, null while it is off. */
  checked: boolean | null;
  tabbable: boolean;
  placeholder: string;
  monoName: boolean;
  monoMeta: boolean;
  anchor: string | undefined;
  /** How the dot draws while the row is off, see ListEntry. */
  dot: ListEntry['dot'];
  /** Wears the pencil of an item you edited, see ListEntry. */
  edited: boolean;
  /** Why the row carries the warn ring, which a reader hears as its
   *  description. Undefined for a row with no ring. */
  warnNote: string | undefined;
  /** The id of the save bar's error line while it names this row, so a
   *  reader hears what to fix. Undefined for a row it does not name. */
  errorId: string | undefined;
  onActivate: (uid: string, e?: MouseEvent<HTMLButtonElement>) => void;
  onMenu: ((uid: string, e: MouseEvent<HTMLButtonElement>) => void) | undefined;
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
  checked,
  tabbable,
  placeholder,
  monoName,
  monoMeta,
  anchor,
  dot,
  edited,
  warnNote,
  errorId,
  onActivate,
  onMenu,
  onFocus,
}: RowProps) {
  const warn = warnNote !== undefined;
  const suggested = !enabled && dot === 'suggested';
  const noteId = warn ? `st-auto-warn-${uid}` : undefined;
  const describedBy = [errorId, noteId].filter(Boolean).join(' ') || undefined;
  return (
    <div className="st-auto-rowwrap">
      <button
        type="button"
        className={cx('st-auto-row', warn && 'is-warn', errorId && 'is-error')}
        // While picking, a row is a toggle that says whether it is
        // checked. Otherwise the open row is the current one.
        aria-current={checked === null && selected ? 'true' : undefined}
        aria-pressed={checked ?? undefined}
        aria-describedby={describedBy}
        tabIndex={tabbable ? 0 : -1}
        data-uid={uid}
        data-st-anchor={anchor}
        data-st-flash={anchor ? '' : undefined}
        onClick={(e) => onActivate(uid, e)}
        onContextMenu={onMenu && ((e) => onMenu(uid, e))}
        onFocus={onFocus}
      >
        {checked !== null && (
          <span className={cx('st-auto-check', checked && 'is-checked')} aria-hidden="true">
            {checked && <CheckIcon size={12} />}
          </span>
        )}
        <span
          className={cx(
            'st-auto-row-name',
            monoName && name && 'st-auto-mono',
            !name && 'st-auto-row-untitled',
          )}
        >
          {name || placeholder}
          {edited && <VisuallyHidden>, edited</VisuallyHidden>}
        </span>
        {meta && <span className={cx('st-auto-row-meta', monoMeta && 'st-auto-mono')}>{meta}</span>}
        {edited && <PencilIcon size={12} className="st-auto-mark" />}
        <span
          className={cx(
            'st-auto-dot dot',
            enabled ? 'is-success' : 'is-off',
            suggested && 'is-accent',
          )}
          aria-hidden="true"
        />
        <VisuallyHidden>
          {enabled ? 'Enabled' : suggested ? 'Suggested, off' : 'Off'}
        </VisuallyHidden>
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
  /** The rows that carry the warn ring, by uid, each with why, like an
   *  alias another group's alias of its name covers. */
  rowNotes?: ReadonlyMap<string, string> | undefined;
  /** The rows the save bar's error names, which carry the danger ring. */
  errorUids?: ReadonlySet<string> | undefined;
  /** The id of the save bar's error line, while it shows. */
  errorId?: string | undefined;
  /** The groups that show folded, by fold key. */
  folded: ReadonlySet<string>;
  /** Fold or open a group from its heading. */
  onFold: (key: string, fold: boolean) => void;
  /** The on and off switch after each group heading. Null or left out
   *  for lists whose groups have none. */
  groupSwitches?: GroupSwitches | null | undefined;
  /** Picking several rows. Null or left out for a list that cannot. */
  picking?: ListPicking | null | undefined;
  /** Open a row's menu, from a right click, the menu key or Shift F10. */
  onRowMenu?: ((uid: string, at: MenuPlacement) => void) | undefined;
  /** Open a group heading's menu, from a right click, the button that
   *  shows on the heading, the menu key or Shift F10. */
  onHeadingMenu?: ((group: string, at: MenuPlacement) => void) | undefined;
  /** The group whose heading shows a name field in its place. */
  renaming?: { group: string; field: ReactNode } | null | undefined;
  /** A note under one group's heading, like the one a rename leaves. */
  headingNote?: { group: string; note: ReactNode } | null | undefined;
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
 *  so the list still takes one Tab from outside.
 *
 *  A list that can pick several rows has Select beside the filter. A
 *  Cmd or Ctrl click, or a Shift click or Shift arrow, picks too. While
 *  picking, each row and group heading wears a checkbox, a click or
 *  Space checks a row, the arrows only move, and Cmd or Ctrl A checks
 *  every row you can see. */
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
  rowNotes,
  errorUids,
  errorId,
  folded,
  onFold,
  groupSwitches,
  picking,
  onRowMenu,
  onHeadingMenu,
  renaming,
  headingNote,
}: ItemListProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const baseId = useId();
  const [cursor, setCursor] = useState<HeadingCursor | null>(null);
  const stops: ListStop[] = listStops(sections, folded);
  // While picking, the arrows pass over the pinned row, which never
  // picks, so they never stop picking on their way.
  if (pinned && !picking?.on) stops.unshift({ kind: 'row', uid: pinned.uid });
  const heading = cursor !== null && cursor.selected === selected ? cursor.id : null;
  const tabId = tabStopId(stops, sections, folded, selected, heading);
  const placeholder = `Untitled ${noun.one}`;
  const onRowFocus = useCallback(() => setCursor(null), []);
  const mac = isMacPlatform();
  const pinnedUid = pinned?.uid ?? null;

  // A click picks while picking is on, or with Cmd or Ctrl, or Shift.
  // The pinned row never picks, so a click on it opens it alone.
  const onActivate = useCallback(
    (uid: string, e?: MouseEvent<HTMLButtonElement>) => {
      if (!picking || uid === pinnedUid) {
        onSelect(uid);
        return;
      }
      const mod = e !== undefined && (mac ? e.metaKey : e.ctrlKey);
      if (e?.shiftKey) picking.row(uid, 'range');
      else if (mod || picking.on) picking.row(uid, 'toggle');
      else onSelect(uid);
    },
    [picking, pinnedUid, mac, onSelect],
  );
  const onMenu = useMemo(
    () =>
      onRowMenu &&
      ((uid: string, e: MouseEvent<HTMLButtonElement>) => {
        if (uid === pinnedUid) return;
        e.preventDefault();
        onRowMenu(uid, { x: e.clientX, y: e.clientY });
      }),
    [onRowMenu, pinnedUid],
  );

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
    const menuKey = e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10');
    // On a heading they open the group's menu under it.
    const headingGroup = fold !== undefined ? groupOfSectionKey(fold) : null;
    if (onHeadingMenu && headingGroup !== null && menuKey) {
      e.preventDefault();
      const r = (e.target as HTMLElement).getBoundingClientRect();
      onHeadingMenu(headingGroup, { x: r.left + 16, y: r.bottom });
      return;
    }
    // The menu key and Shift F10 open a row's menu under the row.
    if (
      onRowMenu &&
      uid !== undefined &&
      uid !== pinnedUid &&
      (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10'))
    ) {
      e.preventDefault();
      const r = (e.target as HTMLElement).getBoundingClientRect();
      onRowMenu(uid, { x: r.left + 16, y: r.bottom });
      return;
    }
    if (picking && (mac ? e.metaKey : e.ctrlKey) && !e.altKey && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      picking.all();
      return;
    }
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
    if (stop.kind === 'row') {
      onSelect(stop.uid);
      // Shift with an arrow checks the run from where you started.
      if (picking && e.shiftKey && stop.uid !== pinnedUid) picking.row(stop.uid, 'range');
    }
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
    <div className={cx('st-auto-list', picking?.on && 'is-picking')}>
      <div className="st-auto-tools">
        <Field
          icon={<SearchIcon />}
          type="search"
          width="auto"
          className="st-auto-filter"
          value={filter}
          onChange={onFilter}
          placeholder={filterLabel}
          aria-label={filterLabel}
        />
        {picking && (
          <IconButton
            className="st-auto-select"
            label={`Select several ${noun.many}`}
            aria-pressed={picking.on}
            icon={<CheckboxIcon />}
            onClick={() => (picking.on ? picking.end() : picking.start())}
          />
        )}
      </div>
      <div ref={scrollRef} className="st-auto-scroll" onKeyDown={onKeyDown}>
        <div className="st-auto-sections">
          {pinned && (
            <ListRow
              uid={pinned.uid}
              name={pinned.name}
              meta={undefined}
              enabled={pinned.enabled}
              selected={selected === pinned.uid}
              checked={null}
              tabbable={tabId === stopId({ kind: 'row', uid: pinned.uid })}
              placeholder={placeholder}
              monoName={false}
              monoMeta={false}
              anchor={pinned.anchor}
              dot={undefined}
              edited={false}
              warnNote={undefined}
              errorId={undefined}
              onActivate={onActivate}
              onMenu={undefined}
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
                    checked={picking?.on ? picking.checked.has(entry.uid) : null}
                    tabbable={tabId === stopId({ kind: 'row', uid: entry.uid })}
                    placeholder={placeholder}
                    monoName={monoName}
                    monoMeta={monoMeta}
                    anchor={entry.anchor}
                    dot={entry.dot}
                    edited={entry.edited ?? false}
                    warnNote={
                      entry.warn ??
                      rowNotes?.get(entry.uid) ??
                      (entry.enabled ? warnNotes?.get(entry.name) : undefined)
                    }
                    errorId={errorUids?.has(entry.uid) ? errorId : undefined}
                    onActivate={onActivate}
                    onMenu={onMenu}
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
            // A group only the draft has gets its switch once you save
            // it. Until then, say when the loadouts would turn it off.
            const newHold =
              group !== null && !groupSwitch && groupSwitches?.newHold?.on === false
                ? groupSwitches.newHold
                : null;
            const noteId = hold || newHold ? `${groupId}-note` : undefined;
            const uids = section.entries.map((e) => e.uid);
            const state = picking?.on ? pickState(picking.checked, uids) : null;
            return (
              <Fragment key={section.key}>
                {divider}
                <div
                  className={cx('st-auto-headrow', index === 0 && !pinned && 'is-first')}
                  onContextMenu={
                    group !== null && onHeadingMenu && renaming?.group !== group
                      ? (e) => {
                          e.preventDefault();
                          onHeadingMenu(group, { x: e.clientX, y: e.clientY });
                        }
                      : undefined
                  }
                >
                  {/* While picking, a group's checkbox checks or clears its
                      rows. It sits over the heading, since it cannot sit
                      inside the fold button. The keys reach the same
                      through Shift arrows and the row menu. */}
                  {state && picking && renaming?.group !== group && (
                    <button
                      type="button"
                      className={cx(
                        'st-auto-check st-auto-headcheck',
                        state === 'all' && 'is-checked',
                        state === 'some' && 'is-some',
                      )}
                      tabIndex={-1}
                      aria-label={`Select every ${noun.one} in ${section.heading}`}
                      aria-pressed={state === 'all' ? true : state === 'some' ? 'mixed' : false}
                      onClick={() => picking.group(uids, state !== 'all')}
                    >
                      {state === 'all' && <CheckIcon size={12} />}
                    </button>
                  )}
                  {renaming && renaming.group === group ? (
                    renaming.field
                  ) : (
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
                  )}
                  {group !== null && onHeadingMenu && renaming?.group !== group && (
                    <IconButton
                      className="st-auto-headmore"
                      label={`${group} group options`}
                      icon={<MoreIcon />}
                      tabIndex={-1}
                      onClick={(e) => {
                        const r = e.currentTarget.getBoundingClientRect();
                        onHeadingMenu(group, { x: r.left, y: r.bottom + 4 });
                      }}
                    />
                  )}
                  {/* The group's on and off switch sits after the heading,
                      since a switch cannot sit inside a button. */}
                  {group !== null && groupSwitch && groupSwitches && (
                    <Toggle
                      className="st-auto-groupswitch"
                      aria-label={`${group} group`}
                      aria-describedby={noteId}
                      checked={groupSwitch.enabled}
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
                {newHold && (
                  <p id={noteId} className="st-auto-groupnote">
                    {newGroupNote(newHold)}
                  </p>
                )}
                {headingNote && headingNote.group === group && (
                  <p className="st-auto-groupnote">{headingNote.note}</p>
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
