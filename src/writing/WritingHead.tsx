import { Fragment, useRef, useState } from 'react';
import type { WritingKind } from '../ipc/writing';
import { openPaneSubmenu, type PaneSubmenuState } from '../panel/affects/affectsDisplay';
import {
  Button,
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CloseIcon,
  IconButton,
  MoreIcon,
  Segmented,
} from '../ui';
import { MenuItem, MenuSeparator, MenuSurface } from '../ui/MenuSurface';
import { menuBelow, submenuAt } from '../ui/menuPlacement';
import type { DraftRow } from './cardMenus';
import { KINDS, SWITCH_LABELS } from './kinds';

// The writing card's header (Note Editor board 1): the kind you write
// as a button that opens what else you can write and your drafts, the
// switch a text about you shares its card through, the line about the
// draft, Guide, the ⋯ menu and Close.

/** What the title's menu lists. */
export interface KindsMenu {
  drafts: DraftRow[];
  boards: WritingKind[];
  aboutYou: WritingKind[];
  sent: DraftRow[];
  others: { key: string; name: string; meta: string }[];
  onDraft: (id: string) => void;
  onNew: (kind: WritingKind) => void;
  onSent: (id: string) => void;
  onOther: (key: string) => void;
}

/** A row of the ⋯ menu. */
export type MoreItem =
  | 'separator'
  | { id: string; label: string; checked?: boolean; disabled?: boolean; run: () => void };

interface Props {
  kind: WritingKind;
  title: string;
  switchKinds: WritingKind[] | null;
  onSwitch: (kind: WritingKind) => void;
  meta: string;
  guide: boolean;
  onGuide: () => void;
  /** Preview stays pressed in the header while it shows, and a press
   *  goes back to writing (Note Editor board 4). */
  preview: boolean;
  onPreview: () => void;
  folded: boolean;
  onUnfold: () => void;
  kinds: KindsMenu;
  more: MoreItem[];
  /** The ⋯ menu's name for a reader. */
  moreLabel: string;
  onClose: () => void;
}

type Sub = 'new' | 'sent' | 'others';

/** The title menu's id, which its submenus place themselves beside. */
const KINDS_MENU = 'wr-kinds';

export function WritingHead({
  kind,
  title,
  switchKinds,
  onSwitch,
  meta,
  guide,
  onGuide,
  preview,
  onPreview,
  folded,
  onUnfold,
  kinds,
  more,
  moreLabel,
  onClose,
}: Props) {
  const titleRef = useRef<HTMLButtonElement | null>(null);
  const [kindsOpen, setKindsOpen] = useState(false);
  const [moreAt, setMoreAt] = useState<HTMLElement | null>(null);
  const [sub, setSub] = useState<PaneSubmenuState<Sub> | null>(null);
  const rowRefs = useRef<Partial<Record<Sub, HTMLButtonElement | null>>>({});

  const closeKinds = () => {
    setKindsOpen(false);
    setSub(null);
  };
  const pick = (run: () => void) => {
    closeKinds();
    run();
  };

  const kindRows = (rows: WritingKind[], select: (k: WritingKind) => void) =>
    rows.map((k) => (
      <MenuItem key={k} onSelect={() => select(k)} onHover={() => setSub(null)}>
        {KINDS[k].title}
      </MenuItem>
    ));

  const draftRow = (row: DraftRow, select: (id: string) => void) => (
    <MenuItem
      key={row.id}
      onSelect={() => select(row.id)}
      onHover={() => setSub(null)}
      trailing={
        <span className="wr-mend">
          <span className="wr-mmeta">{row.meta}</span>
          {row.open && <CheckIcon className="wr-mcheck" />}
        </span>
      }
    >
      <span className="wr-mkind">{KINDS[row.kind].title}</span>
      <span className="wr-mtitle">{row.title}</span>
    </MenuItem>
  );

  const subRow = (which: Sub, label: string, meta?: string) => (
    <MenuItem
      key={which}
      itemRef={(el) => {
        rowRefs.current[which] = el;
      }}
      submenu={{
        open: sub?.which === which,
        controls: `wr-sub-${which}`,
        onOpen: (focus) => setSub((prev) => openPaneSubmenu(prev, which, focus)),
      }}
      trailing={
        <span className="wr-mend">
          {meta && <span className="wr-mmeta">{meta}</span>}
          <ChevronRightIcon className="pane-menu-chevron" />
        </span>
      }
    >
      {label}
    </MenuItem>
  );

  const newRows = (
    <>
      {kindRows(kinds.boards, (k) => pick(() => kinds.onNew(k)))}
      <MenuSeparator />
      <li role="presentation" className="wr-mhead">
        About you
      </li>
      {kindRows(kinds.aboutYou, (k) => pick(() => kinds.onNew(k)))}
    </>
  );

  let submenu = null;
  const subRowEl = sub ? rowRefs.current[sub.which] : null;
  const menuEl = document.getElementById(KINDS_MENU);
  if (sub && subRowEl && menuEl) {
    const at = submenuAt(subRowEl.getBoundingClientRect(), menuEl.getBoundingClientRect());
    submenu = (
      <MenuSurface
        id={`wr-sub-${sub.which}`}
        label={sub.which === 'new' ? 'New' : sub.which === 'sent' ? 'Sent' : 'Other characters'}
        nested
        autoFocus={sub.focus}
        className={`pane-menu-sub${sub.which === 'new' ? '' : ' wr-menu-wide'}`}
        at={at}
        onClose={() => setSub(null)}
      >
        {sub.which === 'new' && newRows}
        {sub.which === 'sent' &&
          kinds.sent.map((row) => draftRow(row, (id) => pick(() => kinds.onSent(id))))}
        {sub.which === 'others' &&
          kinds.others.map((o) => (
            <MenuItem
              key={o.key}
              onSelect={() => pick(() => kinds.onOther(o.key))}
              trailing={<span className="wr-mmeta">{o.meta}</span>}
            >
              {o.name}
            </MenuItem>
          ))}
      </MenuSurface>
    );
  }

  const hasDrafts = kinds.drafts.length > 0;
  const titleButton = (
    <button
      ref={titleRef}
      type="button"
      className={`wr-kind${kindsOpen ? ' is-open' : ''}`}
      aria-haspopup="menu"
      aria-expanded={kindsOpen}
      onClick={() => (kindsOpen ? closeKinds() : setKindsOpen(true))}
    >
      <h2 className="pc-title">{title}</h2>
      <ChevronDownIcon size={12} />
    </button>
  );

  return (
    <div
      className={`pc-head wr-head${folded ? ' is-folded' : ''}`}
      onClick={(e) => {
        if (folded && e.target === e.currentTarget) onUnfold();
      }}
    >
      {titleButton}
      {switchKinds && (
        <Segmented
          label="Which text"
          className="wr-switch"
          options={switchKinds.map((k) => ({ value: k, label: SWITCH_LABELS[k] ?? k }))}
          value={kind}
          onChange={onSwitch}
        />
      )}
      <span className="pc-saved">{meta}</span>
      <span className="pc-spacer" />
      {folded ? (
        <IconButton label="Unfold the card" icon={<ChevronDownIcon />} onClick={onUnfold} />
      ) : (
        <>
          {preview && (
            <Button className="wr-small is-pressed" aria-pressed onClick={onPreview}>
              Preview
            </Button>
          )}
          <Button
            className={`wr-small${guide ? ' is-pressed' : ''}`}
            aria-pressed={guide}
            onClick={onGuide}
          >
            Guide
          </Button>
          <IconButton
            label={moreLabel}
            icon={<MoreIcon />}
            aria-haspopup="menu"
            aria-expanded={moreAt !== null}
            onClick={(e) => setMoreAt(moreAt ? null : e.currentTarget)}
          />
        </>
      )}
      <IconButton label="Close" icon={<CloseIcon />} onClick={onClose} />
      {kindsOpen && titleRef.current && (
        <MenuSurface
          id={KINDS_MENU}
          label="What you write"
          at={menuBelow(titleRef.current.getBoundingClientRect())}
          anchor={titleRef.current}
          {...(hasDrafts ? { className: 'wr-menu-wide' } : {})}
          onClose={closeKinds}
        >
          {hasDrafts ? (
            <>
              <li role="presentation" className="wr-mhead">
                Drafts
              </li>
              {kinds.drafts.map((row) => draftRow(row, (id) => pick(() => kinds.onDraft(id))))}
              <MenuSeparator />
              {subRow('new', 'New')}
            </>
          ) : (
            newRows
          )}
          {(kinds.sent.length > 0 || kinds.others.length > 0) && !hasDrafts && <MenuSeparator />}
          {kinds.sent.length > 0 && subRow('sent', 'Sent', String(kinds.sent.length))}
          {kinds.others.length > 0 && subRow('others', 'Other characters')}
        </MenuSurface>
      )}
      {submenu}
      {moreAt && (
        <MenuSurface
          label={moreLabel}
          at={menuBelow(moreAt.getBoundingClientRect())}
          anchor={moreAt}
          className="wr-menu-more"
          onClose={() => setMoreAt(null)}
        >
          {more.map((item, i) =>
            item === 'separator' ? (
              <MenuSeparator key={`sep-${i}`} />
            ) : (
              <Fragment key={item.id}>
                <MenuItem
                  disabled={item.disabled === true}
                  {...(item.checked !== undefined ? { checked: item.checked } : {})}
                  {...(item.checked ? { trailing: <CheckIcon /> } : {})}
                  onSelect={() => {
                    setMoreAt(null);
                    item.run();
                  }}
                >
                  {item.label}
                </MenuItem>
              </Fragment>
            ),
          )}
        </MenuSurface>
      )}
    </div>
  );
}
