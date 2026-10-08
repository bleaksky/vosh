import type { Draft, WritingCharacter, WritingFile, WritingKind } from '../ipc/writing';
import { KINDS } from './kinds';
import { count } from './text';

// The writing card's two menus as rows: the title's, which lists your
// drafts, the kinds under New, your posts under Sent and the other
// characters with drafts, and the ⋯ menu, which differs between a note
// and a text the game saves in place.

/** A row of the title's menu that opens one draft or one post. */
export interface DraftRow {
  id: string;
  kind: WritingKind;
  title: string;
  meta: string;
  open?: boolean;
}

/** A row of the ⋯ menu, which the card gives its action. */
export type MoreAction =
  | 'language'
  | 'race'
  | 'rewrap'
  | 'preview'
  | 'spelling'
  | 'copy'
  | 'copy-draft'
  | 'delete'
  | 'read'
  | 'restore'
  | 'check'
  | 'clear';

export type MoreRow =
  | 'separator'
  | { id: MoreAction; label: string; checked?: boolean; disabled?: boolean };

/** The ⋯ menu. A note's keeps the description card's Rewrap all, Check
 *  spelling and Copy all, and adds a language, Copy to a new draft and
 *  Delete the draft…. Read again and Restore are a text about you's,
 *  since the game holds no copy of a note to go back to, and
 *  so is the check, which you send whenever you like. */
export function moreRows(m: {
  kind: WritingKind;
  /** The note carries a Language row now. */
  language: boolean;
  customRace: boolean;
  spelling: boolean;
  sentView: boolean;
  /** Read again can run: you play the character and no job runs. */
  canRead: boolean;
  /** The draft differs from the game's copy the card knows. */
  canRestore: boolean;
  /** The game holds the text as the card shows it, so a check reads it. */
  canCheck: boolean;
}): MoreRow[] {
  const info = KINDS[m.kind];
  const spelling: MoreRow = { id: 'spelling', label: 'Check spelling', checked: m.spelling };
  if (info.board) {
    return [
      ...(info.language
        ? [
            {
              id: 'language' as const,
              label: m.language ? 'Write in Common' : 'Write in a language',
            },
          ]
        : []),
      ...(m.kind === 'application'
        ? [{ id: 'race' as const, label: 'Custom race application', checked: m.customRace }]
        : []),
      { id: 'rewrap', label: 'Rewrap all' },
      { id: 'preview', label: 'Preview as readers see it' },
      'separator',
      spelling,
      'separator',
      { id: 'copy', label: 'Copy all' },
      { id: 'copy-draft', label: 'Copy to a new draft' },
      { id: 'delete', label: 'Delete the draft…', disabled: m.sentView },
    ];
  }
  return [
    { id: 'read', label: 'Read again from the game', disabled: !m.canRead },
    { id: 'rewrap', label: 'Rewrap all' },
    { id: 'preview', label: 'Preview as a looker sees it' },
    { id: 'restore', label: 'Put back what the game had', disabled: !m.canRestore },
    ...(info.check
      ? [
          {
            id: 'check' as const,
            label: m.kind === 'description' ? 'Send for approval…' : 'Send for review…',
            disabled: !m.canCheck,
          },
        ]
      : []),
    'separator',
    spelling,
    'separator',
    { id: 'copy', label: 'Copy all' },
    { id: 'clear', label: 'Clear the draft…' },
  ];
}

/** What a draft's row names it by: a note's subject, or its first line. */
function titleOf(d: Draft): string {
  return (KINDS[d.kind].board ? d.subject : '') || d.text.find((l) => l.trim()) || 'Empty';
}

/** Your notes in progress for the title's menu, newest first, the open
 *  one checked. A text about you keeps one draft, which New opens. */
export function draftRows(drafts: readonly Draft[], open: string): DraftRow[] {
  return drafts
    .filter((d) => KINDS[d.kind].board)
    .map((d) => ({
      id: d.id,
      kind: d.kind,
      title: titleOf(d),
      meta: `${count(d.text, 75).lines} lines`,
      open: d.id === open,
    }));
}

/** Your posts under Sent, with the day each posted. */
export function sentRows(sent: readonly Draft[]): DraftRow[] {
  return sent.map((d) => ({
    id: d.id,
    kind: d.kind,
    title: titleOf(d),
    meta: new Date(d.at).toLocaleDateString('en-US', { month: 'short', day: 'numeric' }),
  }));
}

/** The other characters with drafts, under Other characters. */
export function otherRows(
  file: WritingFile,
  you: WritingCharacter | null,
): { key: string; name: string; meta: string }[] {
  return Object.entries(file.characters)
    .filter(
      ([, other]) =>
        other.drafts.length > 0 &&
        !(you && other.name === you.name && other.host === you.host && other.port === you.port),
    )
    .map(([key, other]) => ({
      key,
      name: other.name,
      meta: `${other.drafts.length} ${other.drafts.length === 1 ? 'draft' : 'drafts'}`,
    }));
}
