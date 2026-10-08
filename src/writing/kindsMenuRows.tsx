import type { ReactElement } from 'react';
import type { WritingKind } from '../ipc/writing';
import { CheckIcon } from '../ui';
import { MenuItem, MenuSeparator } from '../ui/MenuSurface';
import type { DraftRow } from './cardMenus';
import { KINDS } from './kinds';
import type { KindsMenu } from './WritingHead';

// The rows of the writing card's title menu (WritingHead.tsx). A row of
// the title menu closes a sibling's submenu when the pointer reaches it.
// A row inside a submenu must not, or the pointer arriving in the
// submenu would close it under itself, and you could never pick from it.

/** The rows of `kinds`, each picking its kind. In the title menu,
 *  pass `closeSub`, which the pointer reaching a row runs to close a
 *  sibling's submenu. In a submenu, pass null: a row there that closed
 *  the submenu would close it under the pointer arriving in it. */
export function kindItems(
  rows: WritingKind[],
  select: (k: WritingKind) => void,
  closeSub: (() => void) | null,
): ReactElement[] {
  return rows.map((k) => (
    <MenuItem key={k} onSelect={() => select(k)} {...(closeSub ? { onHover: closeSub } : {})}>
      {KINDS[k].title}
    </MenuItem>
  ));
}

/** What you can start: the boards, then the texts about you. In the
 *  title menu when you have no drafts, or in its New submenu.
 *  `closeSub` as kindItems takes it. */
export function newKindRows(
  kinds: Pick<KindsMenu, 'boards' | 'aboutYou'>,
  select: (k: WritingKind) => void,
  closeSub: (() => void) | null,
): ReactElement {
  return (
    <>
      {kindItems(kinds.boards, select, closeSub)}
      <MenuSeparator />
      <li role="presentation" className="wr-mhead">
        About you
      </li>
      {kindItems(kinds.aboutYou, select, closeSub)}
    </>
  );
}

/** A draft or a post, with its kind, title and age. `closeSub` as
 *  kindItems takes it. */
export function draftItem(
  row: DraftRow,
  select: (id: string) => void,
  closeSub: (() => void) | null,
): ReactElement {
  return (
    <MenuItem
      key={row.id}
      onSelect={() => select(row.id)}
      {...(closeSub ? { onHover: closeSub } : {})}
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
}
