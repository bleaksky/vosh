import type { Draft, WritingCharacter, WritingFile, WritingKind } from '../ipc/writing';
import { type Ask, CLEAR_ASK, DELETE_ASK } from './cardDialogs';
import { draftRows, moreRows, otherRows, sentRows, type MoreAction } from './cardMenus';
import {
  characterOf,
  getWritingFile,
  keepCharacter,
  keepSwitches,
  newDraft,
  withoutDraft,
  type World,
} from './draftsStore';
import { KINDS, writable } from './kinds';
import { sameLines } from './text';
import type { WritingJobs } from './useWritingJob';
import type { KindsMenu, MoreItem } from './WritingHead';

// The writing card's two header menus: the kinds menu under its title,
// with your drafts, the boards you write on, your posts and other
// characters' drafts, and the More menu with what the card does to the
// text in it.

/** What the header menus read from the writing card and change on it. */
export interface MenuCard {
  character: WritingCharacter | null;
  draft: Draft;
  level: number | null;
  file: WritingFile;
  /** The room the session stands in, which a new report starts in. */
  roomName: string | null;
  kind: WritingKind;
  lines: string[];
  world: World | null;
  name: string | null;
  sentView: boolean;
  live: boolean;
  running: WritingJobs['running'];
  canCheck: boolean;
  openDraft: (k: WritingKind) => Draft;
  switchTo: (k: WritingKind, d?: Draft) => void;
  readIfNoDraft: (k: WritingKind, d: Draft) => void;
  keep: (next: Draft) => void;
  show: (lines: readonly string[]) => void;
  rewrapEvery: () => void;
  readAgain: () => void;
  check: () => void;
  setSentView: (on: boolean) => void;
  setOther: (other: { world: World; name: string }) => void;
  setPreview: (on: boolean) => void;
  setConfirm: (confirm: Ask & { run: () => void }) => void;
}

/** The kinds menu under the card's title. */
export function kindsMenuFor(card: MenuCard): KindsMenu {
  const { character, draft, level, file, roomName, openDraft, switchTo, readIfNoDraft } = card;
  const { setSentView, setOther } = card;
  return {
    drafts: draftRows(character?.drafts ?? [], draft.id),
    boards: writable(level),
    aboutYou: ['description', 'history'],
    sent: sentRows(character?.sent ?? []),
    others: otherRows(file, character),
    onDraft: (id) => {
      const d = character?.drafts.find((x) => x.id === id);
      if (d) switchTo(d.kind, d);
    },
    onNew: (k) => {
      const d = KINDS[k].board ? newDraft(k, KINDS[k].room ? roomName : null) : openDraft(k);
      switchTo(k, d);
      readIfNoDraft(k, d);
    },
    onSent: (id) => {
      const d = character?.sent.find((x) => x.id === id);
      if (!d) return;
      switchTo(d.kind, d);
      setSentView(true);
    },
    onOther: (key) => {
      const them = file.characters[key];
      if (!them) return;
      setOther({ world: { host: them.host, port: them.port }, name: them.name });
      const d = them.drafts.find((x) => KINDS[x.kind].board) ?? them.drafts[0];
      if (d) switchTo(d.kind, d);
    },
  };
}

/** The More menu's rows, each with what it does. */
export function moreItemsFor(card: MenuCard): MoreItem[] {
  const { draft, file, kind, lines, world, name, sentView, live, running, canCheck } = card;
  const { switchTo, keep, show, rewrapEvery, readAgain, check, setPreview, setConfirm } = card;
  const hasLanguage = draft.language !== null && draft.language !== undefined;
  const moreActions: Record<MoreAction, () => void> = {
    language: () => keep({ ...draft, language: hasLanguage ? null : '' }),
    race: () => keep({ ...draft, custom_race: !draft.custom_race }),
    rewrap: rewrapEvery,
    preview: () => setPreview(true),
    spelling: () => keepSwitches(!file.spelling, file.guide),
    copy: () => void navigator.clipboard.writeText(lines.join('\n')).catch(() => {}),
    'copy-draft': () =>
      switchTo(kind, {
        ...draft,
        ...newDraft(kind),
        to: draft.to ?? '',
        subject: draft.subject ?? '',
        text: lines,
      }),
    delete: () =>
      setConfirm({
        ...DELETE_ASK,
        run: () => {
          if (world && name)
            keepCharacter(withoutDraft(characterOf(getWritingFile(), world, name), draft.id));
          switchTo(kind, newDraft(kind));
        },
      }),
    read: readAgain,
    check,
    restore: () => {
      if (!draft.game) return;
      show(draft.game);
      keep({ ...draft, text: draft.game });
    },
    clear: () =>
      setConfirm({
        ...CLEAR_ASK,
        run: () => {
          show([]);
          keep({ ...draft, text: [] });
        },
      }),
  };
  return moreRows({
    kind,
    language: hasLanguage,
    customRace: draft.custom_race === true,
    spelling: file.spelling,
    sentView,
    canRead: live && running === null,
    canRestore: !!draft.game && !sameLines(lines, draft.game),
    canCheck,
  }).map((row) => (row === 'separator' ? row : { ...row, run: moreActions[row.id] }));
}
