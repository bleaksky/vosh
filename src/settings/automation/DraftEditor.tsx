import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  addDraftItem,
  countPhrase,
  createDraft,
  discardDraft,
  discardTitle,
  draftChangeCount,
  draftValues,
  findDraftItem,
  isDraftDirty,
  listChangedNote,
  markAllWritten,
  nextDraftUid,
  removeDraftItem,
  replaceDraftValues,
  saveListThenPinned,
  storeChangeAction,
  updateDraftItem,
  type Draft,
  type SavedWrite,
  type SaveProblem,
} from '../../automation/automationDraft';
import {
  buildSections,
  filterSections,
  isFiltering,
  neighborUid,
  sectionKeyOf,
  sectionOrder,
  visibleOrder,
  type ListEntry,
} from '../../automation/automationList';
import { automationSaveError } from '../../automation/automationRecords';
import { scrollWithin } from '../../lib/scrollWithin';
import { Button } from '../../ui';
import { getShownProfile, isShownHeld, subscribeShownMoves, useProfileHold } from '../shownProfile';
import { ItemList, type PinnedEntry } from './ItemList';
import { JsonPanel } from './JsonPanel';
import { SaveBar, type SaveStatus } from './SaveBar';
import type { DirtyReport, KindSpec } from './types';
import { useGroupSwitches } from './useGroupSwitches';
import { useListFolds } from './useListFolds';

/** A block pinned above the list with its own detail and its own
 *  draft, like the Tick in Timers. Save and Discard cover it too. */
export interface PinnedPart extends PinnedEntry {
  dirty: boolean;
  /** How the discard dialog names it, like `the tick`. */
  phrase: string;
  validate?: () => string | null;
  save: () => Promise<void>;
  discard: () => void;
  render: () => ReactNode;
}

interface DraftEditorProps<T> {
  spec: KindSpec<T>;
  /** The JSON view is open. */
  json: boolean;
  onJson: (open: boolean) => void;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
  pinned?: PinnedPart | null;
  /** Select the pinned block each time this goes up. */
  pinnedSeq?: number;
  /** Select the item with this key, by linkKeyOf or else keyOf, and
   *  bring its row into view each time `seq` goes up, as a deep link
   *  asks. */
  selectKey?: { key: string; seq: number } | null;
  /** Set the filter to `text` each time `seq` goes up, as a link asks. */
  filterTo?: { text: string; seq: number } | null;
  /** More on the left of the save bar, given the draft and its setter. */
  barExtra?: (draft: Draft<T>, setDraft: (next: Draft<T>) => void) => ReactNode;
  /** The list rows that carry the warn ring while they are on, by name,
   *  each with why, for a reader. */
  warnNotes?: ReadonlyMap<string, string>;
}

const SAVED_MS = 2000;
const JSON_PARSE_MS = 150;

/** What stopped the last Save, as the save bar shows it. `uids` are the
 *  rows the list marks. `check` is set for a problem the list itself
 *  has, which goes away as you fix it. */
interface SaveError {
  message: string;
  uids: readonly string[];
  check: boolean;
}

const NO_UIDS: readonly string[] = [];

/** `problem` about the draft values, with the uids of the items it
 *  names, in list order. */
function errorOf<T>(problem: SaveProblem, draft: Draft<T>): SaveError {
  const uids = problem.at.flatMap((at) => {
    const item = draft.items[at];
    return item ? [item.uid] : [];
  });
  return { message: problem.message, uids, check: true };
}

/** One kind's list, detail card, and save bar over a draft. The draft
 *  loads when the editor mounts, from the profile Settings shows. Save
 *  validates, writes it through the kind's API to that profile, and
 *  loads it again, so the page shows what the store kept. Discard puts
 *  the last load back, or the list as the store holds it now when it
 *  changed elsewhere while you edited. Unsaved changes hold the profile
 *  (shownProfile.ts), so a selection that brings another profile to the
 *  front waits for Save or Discard, and the list loads that profile's
 *  once Settings moves. */
export function DraftEditor<T>({
  spec,
  json,
  onJson,
  onDirty,
  onError,
  pinned = null,
  pinnedSeq = 0,
  selectKey = null,
  filterTo = null,
  barExtra,
  warnNotes,
}: DraftEditorProps<T>) {
  const [draft, setDraftState] = useState<Draft<T> | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [fresh, setFresh] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [busy, setBusy] = useState(false);
  const [justSaved, setJustSaved] = useState(false);
  const [revealSeq, setRevealSeq] = useState(0);
  const [jsonText, setJsonText] = useState('');
  const [jsonBad, setJsonBad] = useState(false);
  const [saveError, setSaveError] = useState<SaveError | null>(null);
  const errorId = useId();
  const draftRef = useRef<Draft<T> | null>(null);
  const selectedRef = useRef<string | null>(null);
  const pinnedUid = pinned?.uid ?? null;
  const pinnedUidRef = useRef<string | null>(pinnedUid);
  const jsonTimer = useRef<number | undefined>(undefined);
  /** A save is writing. Store changes wait for the load that ends it. */
  const savingRef = useRef(false);
  /** Counts each load, so only the newest lands. */
  const loadsRef = useRef(0);
  /** The store changed while the draft held unsaved edits, so the last
   *  load no longer matches it. */
  const staleRef = useRef(false);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  /** Goes up after each delete, so focus moves once the list redraws. */
  const [deleteSeq, setDeleteSeq] = useState(0);
  const afterDeleteRef = useRef<string | null>(null);
  const folds = useListFolds(spec.id, filter);
  const groupSwitches = useGroupSwitches(spec.groups ?? null, draft?.saved, onError);
  /** The selection when the filter took its first letter. */
  const filterFromRef = useRef<string | null>(null);

  const setDraft = useCallback((next: Draft<T> | null) => {
    draftRef.current = next;
    setDraftState(next);
  }, []);

  useEffect(() => {
    selectedRef.current = selected;
  }, [selected]);

  useEffect(() => {
    pinnedUidRef.current = pinnedUid;
  }, [pinnedUid]);

  const selectedKey = useCallback((): string | null => {
    const d = draftRef.current;
    const uid = selectedRef.current;
    if (!d || uid === null) return null;
    const item = findDraftItem(d, uid);
    return item ? spec.keyOf(item.value) : null;
  }, [spec]);

  // Load `profile`'s list, the profile Settings shows unless named, and
  // keep the selection on the same item by its natural key.
  const load = useCallback(
    async (keepKey: string | null, profile: string | undefined = getShownProfile()) => {
      const mine = ++loadsRef.current;
      const values = await spec.load(profile);
      if (mine !== loadsRef.current) return;
      const next = createDraft(values);
      staleRef.current = false;
      setDraft(next);
      if (spec.json) setJsonText(spec.json.toText(values));
      setJsonBad(false);
      setSelected((prev) => {
        if (prev !== null && prev === pinnedUidRef.current) return prev;
        if (keepKey === null) return null;
        return next.items.find((item) => spec.keyOf(item.value) === keepKey)?.uid ?? null;
      });
    },
    [spec, setDraft],
  );

  useEffect(() => {
    let cancelled = false;
    load(null).catch((e) => {
      if (!cancelled) onError(automationSaveError(e));
    });
    return () => {
      cancelled = true;
    };
  }, [load, onError]);

  // Settings moved to another profile, so load its list. Unsaved
  // changes held the profile until now, so the draft is clean.
  useEffect(
    () =>
      subscribeShownMoves(() => {
        void load(selectedKey()).catch((e) => onError(automationSaveError(e)));
      }),
    [load, selectedKey, onError],
  );

  // The store changed outside the page, like #trigger in the main
  // window or a script. Follow it while the draft is clean. With unsaved
  // changes, keep them and say so. Save applies them over the new list.
  // While Settings holds its profile the change is to the profile in
  // front, another one.
  useEffect(() => {
    if (!spec.subscribe) return;
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void spec
      .subscribe(() => {
        const d = draftRef.current;
        if (cancelled || !d || isShownHeld()) return;
        const action = storeChangeAction({ dirty: isDraftDirty(d), saving: savingRef.current });
        if (action === 'reload') {
          void load(selectedKey()).catch(() => {});
        } else if (action === 'warn') {
          staleRef.current = true;
          onError(listChangedNote(spec.noun));
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unsub = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [spec, load, selectedKey, onError]);

  const entryCache = useMemo(() => new WeakMap<object, Omit<ListEntry, 'uid'>>(), []);
  const entries = useMemo<ListEntry[]>(() => {
    if (!draft) return [];
    return draft.items.map((item) => {
      const key = item.value as unknown as object;
      let entry = entryCache.get(key);
      if (!entry) {
        entry = spec.entry(item.value);
        entryCache.set(key, entry);
      }
      return { uid: item.uid, ...entry };
    });
  }, [draft, spec, entryCache]);
  // The warn notes of the rows, by uid, like an alias another group's
  // alias of its name covers.
  const byName = groupSwitches?.byName;
  const rowNotes = useMemo(() => {
    const notes = new Map<string, string>();
    if (!draft || !spec.rowNotes) return notes;
    const values = draft.items.map((item) => item.value);
    const groupOn = (group: string) => byName?.get(group)?.enabled ?? true;
    spec.rowNotes(values, groupOn).forEach((note, at) => {
      if (note) notes.set(draft.items[at].uid, note);
    });
    return notes;
  }, [draft, spec, byName]);
  const errorUids = useMemo(() => new Set(saveError?.uids ?? NO_UIDS), [saveError]);

  // A problem the list has stays in the save bar while you fix it, and
  // follows your edits: it names what is still wrong, and goes once
  // nothing is.
  useEffect(() => {
    if (!draft || !saveError?.check) return;
    const problem = spec.validate?.(draftValues(draft)) ?? null;
    if (!problem) {
      setSaveError(null);
      return;
    }
    const next = errorOf(problem, draft);
    if (
      next.message !== saveError.message ||
      next.uids.length !== saveError.uids.length ||
      next.uids.some((uid, i) => uid !== saveError.uids[i])
    ) {
      setSaveError(next);
    }
  }, [draft, saveError, spec]);

  const allSections = useMemo(() => buildSections(entries), [entries]);
  const sections = useMemo(() => filterSections(allSections, filter), [allSections, filter]);
  // The rows you can see. A folded group's rows leave it.
  const order = useMemo(
    () => [...(pinned ? [pinned.uid] : []), ...visibleOrder(sections, folds.folded)],
    [pinned, sections, folds.folded],
  );

  // Keep a selection when none is set or the selected item is gone: the
  // first row you can see, else the first row a folded group hides. A
  // selection then exists whenever a row does, so opening a group never
  // picks one, and the heading that hides it takes Tab.
  useEffect(() => {
    if (!draft) return;
    const exists =
      selected !== null &&
      ((pinned !== null && selected === pinned.uid) || findDraftItem(draft, selected));
    if (!exists) setSelected(order[0] ?? sectionOrder(sections)[0] ?? null);
  }, [draft, selected, order, pinned, sections]);

  // Select the pinned block once per request, as soon as it has loaded.
  const pinnedSeqDone = useRef(0);
  useEffect(() => {
    if (!pinnedSeq || pinnedUid === null || pinnedSeqDone.current === pinnedSeq) return;
    pinnedSeqDone.current = pinnedSeq;
    setSelected(pinnedUid);
  }, [pinnedSeq, pinnedUid]);

  // Select the item a link names once per request, as soon as the list
  // has loaded.
  const selectSeqDone = useRef(0);
  useEffect(() => {
    if (!selectKey || !draft || selectSeqDone.current === selectKey.seq) return;
    selectSeqDone.current = selectKey.seq;
    const linkKey = spec.linkKeyOf ?? spec.keyOf;
    const item = draft.items.find((i) => linkKey(i.value) === selectKey.key);
    if (!item) return;
    setFilter('');
    setSelected(item.uid);
    reveal(item.uid);
    // reveal reads the draft through its ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectKey, draft, spec]);

  // Set the filter a link names once per request.
  const filterSeqDone = useRef(0);
  useEffect(() => {
    if (!filterTo || filterSeqDone.current === filterTo.seq) return;
    filterSeqDone.current = filterTo.seq;
    setFilter(filterTo.text);
  }, [filterTo]);

  const count = draft ? draftChangeCount(draft) : 0;

  // The list went clean while it was behind the store, after Discard or
  // after you undid your edits. Catch up with the store now.
  useEffect(() => {
    if (count > 0 || !staleRef.current) return;
    staleRef.current = false;
    onError(null);
    void load(selectedKey()).catch((e) => onError(automationSaveError(e)));
  }, [count, load, selectedKey, onError]);

  const pinnedDirty = pinned?.dirty ?? false;
  const dirty = count > 0 || pinnedDirty;
  useProfileHold(dirty);
  const title = dirty
    ? discardTitle([
        count > 0 ? countPhrase(count, spec.noun) : '',
        pinnedDirty && pinned ? pinned.phrase : '',
      ])
    : null;

  useEffect(() => {
    onDirty(title ? { title, body: 'Vosh keeps what you saved last.' } : null);
  }, [title, onDirty]);
  useEffect(() => () => onDirty(null), [onDirty]);

  useEffect(() => {
    if (dirty) setJustSaved(false);
  }, [dirty]);
  useEffect(() => {
    if (!justSaved) return;
    const id = window.setTimeout(() => setJustSaved(false), SAVED_MS);
    return () => window.clearTimeout(id);
  }, [justSaved]);

  useEffect(() => () => window.clearTimeout(jsonTimer.current), []);

  const onSelect = useCallback((uid: string) => setSelected(uid), []);

  const update = (uid: string) => (fn: (value: T) => T) => {
    const d = draftRef.current;
    if (d) setDraft(updateDraftItem(d, uid, fn));
  };

  /** Open the group that holds an item, as the draft has it now. */
  const openGroupOf = (uid: string) => {
    const d = draftRef.current;
    const item = d ? findDraftItem(d, uid) : undefined;
    if (item) folds.open(sectionKeyOf(spec.entry(item.value)));
  };

  /** Bring an item's row into view, opening its group when folded. */
  const reveal = (uid: string) => {
    openGroupOf(uid);
    setRevealSeq((n) => n + 1);
  };

  // While the filter has text every group with a match shows open.
  // Clearing it folds them again, all but the group of a row you picked
  // from the matches, which stays open so its row still shows.
  const onFilter = (value: string) => {
    const was = isFiltering(filter);
    const now = isFiltering(value);
    if (!was && now) filterFromRef.current = selected;
    if (was && !now && selected !== null && selected !== filterFromRef.current) {
      openGroupOf(selected);
    }
    setFilter(value);
  };

  const onNew = () => {
    const d = draftRef.current;
    if (!d || !spec.blank) return;
    const uid = nextDraftUid();
    setDraft(addDraftItem(d, spec.blank(), uid));
    setFilter('');
    setSelected(uid);
    setFresh(uid);
    reveal(uid);
  };

  const onDelete = (uid: string) => {
    const d = draftRef.current;
    if (!d) return;
    const rows = order.filter((u) => !pinned || u !== pinned.uid);
    // With no row left beside it, the pinned block takes the selection.
    const next = neighborUid(rows, uid) ?? pinned?.uid ?? null;
    setSelected(next);
    setDraft(removeDraftItem(d, uid));
    afterDeleteRef.current = next;
    setDeleteSeq((n) => n + 1);
  };

  // Delete removes the card that held focus, the button included. Put
  // focus on the row that took the deleted one's place, else the row
  // that is selected, else the filter, so it never drops to the page.
  useEffect(() => {
    if (deleteSeq === 0) return;
    const body = bodyRef.current;
    if (!body) return;
    const uid = afterDeleteRef.current;
    const row =
      (uid !== null
        ? body.querySelector<HTMLElement>(`.st-auto-row[data-uid="${CSS.escape(uid)}"]`)
        : null) ?? body.querySelector<HTMLElement>('.st-auto-row[aria-current]');
    const target = row ?? body.querySelector<HTMLElement>('.st-auto-filter input');
    // Focus alone would scroll every box above the target. Move only
    // the box that holds it.
    target?.focus({ preventScroll: true });
    scrollWithin(target, { block: 'nearest' });
  }, [deleteSeq]);

  // JSON edits reach the draft after a short pause. Take the text
  // still waiting now, or say it does not read.
  const jsonPending = useRef<string | null>(null);
  const applyJson = (text: string): boolean => {
    jsonPending.current = null;
    window.clearTimeout(jsonTimer.current);
    const d = draftRef.current;
    const values = d ? (spec.json?.fromText(text, draftValues(d)) ?? null) : null;
    if (!values || !d) {
      setJsonBad(true);
      return false;
    }
    setJsonBad(false);
    setDraft(replaceDraftValues(d, values, spec.keyOf));
    return true;
  };

  const onDiscard = () => {
    jsonPending.current = null;
    window.clearTimeout(jsonTimer.current);
    const d = draftRef.current;
    if (d) {
      const restored = discardDraft(d);
      setDraft(restored);
      if (spec.json) setJsonText(spec.json.toText(draftValues(restored)));
    }
    setJsonBad(false);
    pinned?.discard();
    setSaveError(null);
    onError(null);
  };

  const onSave = async () => {
    if (jsonPending.current !== null && !applyJson(jsonPending.current)) return;
    const d = draftRef.current;
    if (!d || busy) return;
    const listProblem = spec.validate?.(draftValues(d)) ?? null;
    if (listProblem) {
      // Mark the rows it names and select the first, so its card shows
      // what to fix.
      const error = errorOf(listProblem, d);
      setSaveError(error);
      const first = error.uids[0];
      if (first !== undefined) {
        setFilter('');
        setSelected(first);
        reveal(first);
      }
      return;
    }
    const pinnedProblem = pinned?.validate?.() ?? null;
    if (pinnedProblem) {
      setSaveError({ message: pinnedProblem, uids: NO_UIDS, check: false });
      return;
    }
    setSaveError(null);
    onError(null);
    setBusy(true);
    savingRef.current = true;
    // The profile Settings shows, which the unsaved changes hold, so the
    // save and the load after it reach it whatever the selection is now.
    const profile = getShownProfile();
    // Each item the store took, for a Save that fails before the list
    // loads again.
    const writes: SavedWrite<T>[] = [];
    let reloaded = false;
    try {
      await saveListThenPinned({
        list: isDraftDirty(d) ? () => spec.save(d, (write) => writes.push(write), profile) : null,
        pinned: pinned?.dirty ? () => pinned.save() : null,
        reload: async () => {
          await load(selectedKey(), profile);
          reloaded = true;
        },
      });
      setJustSaved(true);
    } catch (e) {
      // Keep your unsaved changes, and mark what the store took as saved
      // so the next Save sends only the rest and makes nothing twice.
      const current = draftRef.current;
      if (!reloaded && current && writes.length > 0) setDraft(markAllWritten(current, writes));
      setSaveError({ message: automationSaveError(e), uids: NO_UIDS, check: false });
    } finally {
      savingRef.current = false;
      setBusy(false);
    }
  };

  const onJsonText = (text: string) => {
    setJsonText(text);
    jsonPending.current = text;
    window.clearTimeout(jsonTimer.current);
    jsonTimer.current = window.setTimeout(() => applyJson(text), JSON_PARSE_MS);
  };

  const openJson = () => {
    const d = draftRef.current;
    if (d && spec.json) setJsonText(spec.json.toText(draftValues(d)));
    setJsonBad(false);
    onJson(true);
  };

  const status: SaveStatus = dirty ? 'dirty' : justSaved ? 'saved' : 'clean';
  const selectedItem = draft && selected !== null ? findDraftItem(draft, selected) : undefined;
  const showingPinned = pinned !== null && selected === pinned.uid;
  const jsonOpen = json && spec.json !== undefined;

  let detail: ReactNode = null;
  if (showingPinned) {
    detail = pinned.render();
  } else if (selectedItem) {
    const uid = selectedItem.uid;
    const canDelete =
      spec.deleteLabel !== undefined && (spec.canDelete?.(selectedItem.value) ?? true);
    detail = (
      <div key={uid} className="st-auto-detail-inner">
        {spec.renderDetail({
          uid,
          value: selectedItem.value,
          update: update(uid),
          fresh: fresh === uid,
          revealInList: () => reveal(uid),
          note: rowNotes.get(uid),
        })}
        {canDelete && (
          <div className="st-auto-detail-actions">
            <Button variant="danger" onClick={() => onDelete(uid)}>
              {spec.deleteLabel}
            </Button>
          </div>
        )}
      </div>
    );
  } else if (draft && (draft.items.length > 0 || pinned)) {
    detail = <p className="st-auto-empty">{spec.emptyDetail}</p>;
  }

  return (
    <>
      <div ref={bodyRef} className="st-auto-body">
        {jsonOpen ? (
          <JsonPanel
            noun={spec.noun}
            text={jsonText}
            bad={jsonBad}
            onChange={onJsonText}
            onDone={() => {
              if (jsonPending.current !== null) applyJson(jsonPending.current);
              onJson(false);
            }}
          />
        ) : (
          <>
            <ItemList
              noun={spec.noun}
              filterLabel={spec.filterLabel}
              filter={filter}
              onFilter={onFilter}
              sections={sections}
              hasItems={(draft?.items.length ?? 0) > 0}
              emptyText={spec.emptyList}
              pinned={pinned}
              selected={selected}
              onSelect={onSelect}
              revealSeq={revealSeq}
              monoName={spec.monoName ?? false}
              monoMeta={spec.monoMeta ?? false}
              warnNotes={warnNotes}
              rowNotes={rowNotes}
              errorUids={errorUids}
              errorId={saveError ? errorId : undefined}
              folded={folds.folded}
              onFold={folds.setFold}
              groupSwitches={groupSwitches}
              footer={
                spec.json ? (
                  <Button
                    className="st-auto-quiet"
                    data-st-anchor="json"
                    data-st-flash=""
                    onClick={openJson}
                  >
                    Edit all as JSON…
                  </Button>
                ) : undefined
              }
            />
            <div className="st-auto-detail">{detail}</div>
          </>
        )}
      </div>
      <SaveBar
        newLabel={jsonOpen ? undefined : spec.newLabel}
        onNew={onNew}
        extra={draft && barExtra ? barExtra(draft, setDraft) : undefined}
        status={status}
        error={saveError?.message}
        errorId={errorId}
        canSave={!(jsonOpen && jsonBad)}
        busy={busy || !draft}
        onDiscard={onDiscard}
        onSave={() => void onSave()}
      />
    </>
  );
}
