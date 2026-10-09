import type { ReactNode } from 'react';
import type { Draft, KindNoun, SavedWrite } from '../../automation/automationDraft';
import type { ListEntry } from '../../automation/automationList';
import type { GroupList } from '../../ipc/automation';

/** The kinds on the Automation switcher. Loadouts shows in loadout
 *  mode only. */
export type AutomationKind = 'triggers' | 'aliases' | 'macros' | 'timers' | 'presets' | 'loadouts';

export const AUTOMATION_KINDS: readonly AutomationKind[] = [
  'triggers',
  'aliases',
  'macros',
  'timers',
  'presets',
  'loadouts',
];

export function isAutomationKind(value: string | undefined): value is AutomationKind {
  return AUTOMATION_KINDS.includes(value as AutomationKind);
}

/** The kinds that edit a list you can also write as JSON. */
export type ListKind = 'triggers' | 'aliases' | 'macros' | 'timers';

export function isListKind(kind: AutomationKind): kind is ListKind {
  return kind === 'triggers' || kind === 'aliases' || kind === 'macros' || kind === 'timers';
}

/** What an editor tells the page while it holds unsaved changes: the
 *  discard dialog's title and body. Null when it is clean. */
export interface DirtyReport {
  title: string;
  body: string;
}

/** The props a detail card gets for the selected item. */
export interface DetailProps<T> {
  uid: string;
  value: T;
  /** Replace the item's value in the draft. */
  update: (fn: (value: T) => T) => void;
  /** True right after New made this item, so the card can focus its
   *  first field. */
  fresh: boolean;
  /** Call after a Group change lands, so the list shows the row under
   *  its new heading. */
  revealInList: () => void;
}

/** Everything one kind's editor needs. Keep a spec stable across
 *  renders (module scope or useMemo), since the editor loads again
 *  when it changes. */
export interface KindSpec<T> {
  /** `triggers`, the name the list keeps its folds under on this
   *  computer. Never shown, so a reworded noun keeps your folds. */
  id: string;
  noun: KindNoun;
  /** `Filter triggers`, the filter field's placeholder and label. */
  filterLabel: string;
  /** `New trigger`. Leave out for kinds you cannot add to. */
  newLabel?: string;
  /** `Delete trigger`. Leave out for kinds you cannot delete from. */
  deleteLabel?: string;
  /** Whether this item can be deleted, when the kind allows it. */
  canDelete?: (value: T) => boolean;
  /** `Choose a trigger to edit it.` */
  emptyDetail: string;
  /** `You have no triggers yet.` */
  emptyList: string;
  /** Read the list of `profile`, the profile Settings shows, or of the
   *  selected session's profile while Settings knows none yet. */
  load: (profile: string | undefined) => Promise<T[]>;
  /** Write the draft to `profile` through the kind's API. A kind that
   *  writes one call per item reports each call the store took to
   *  `written`, so a Save that fails partway marks those items saved and
   *  the next Save does not send them again. */
  save: (
    draft: Draft<T>,
    written: (write: SavedWrite<T>) => void,
    profile: string | undefined,
  ) => Promise<void>;
  /** Why the draft cannot save yet, or null. */
  validate?: (values: T[]) => string | null;
  /** The list row for a value, less its uid. */
  entry: (value: T) => Omit<ListEntry, 'uid'>;
  /** A stable name for an item across a reload, like a trigger's name. */
  keyOf: (value: T) => string;
  blank?: () => T;
  /** JSON editing. Leave out for kinds with no JSON view. */
  json?: {
    toText: (values: T[]) => string;
    /** The values, or null when the text does not read. `current` is
     *  the draft as it stands, for a kind whose text leaves some items
     *  out, as Triggers leaves out the preset triggers. */
    fromText: (text: string, current: readonly T[]) => T[] | null;
  };
  /** Reload when the store changes elsewhere while the draft is clean. */
  subscribe?: (onChange: () => void) => Promise<() => void>;
  renderDetail: (props: DetailProps<T>) => ReactNode;
  /** The list whose groups the switch on each heading turns. Leave out
   *  for kinds whose headings have no switch, like Presets. */
  groups?: GroupList;
  /** Mono list rows for MUD text, like alias names and macro keys. */
  monoName?: boolean;
  monoMeta?: boolean;
}

/** What the page hands a kind's editor. */
export interface EditorProps {
  /** The JSON view is open. */
  json: boolean;
  onJson: (open: boolean) => void;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
}

/** Where a link on a preset's card opens Triggers: on one trigger, by
 *  name, or with the filter set, as to a preset's name. `seq` goes up
 *  with each link, so the same one opens again. */
export interface TriggersLink {
  select?: string;
  filter?: string;
  seq: number;
}
