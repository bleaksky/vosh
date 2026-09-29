import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
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
  nextDraftUid,
  removeDraftItem,
  replaceDraftValues,
  saveListThenPinned,
  storeChangeAction,
  updateDraftItem,
  type Draft,
} from '../../../../lib/automationDraft';
import {
  buildSections,
  filterSections,
  neighborUid,
  sectionOrder,
  type ListEntry,
} from '../../../../lib/automationList';
import { automationSaveError } from '../../../../lib/automationRecords';
import { subscribeProfileSwitched } from '../../../../lib/session';
import { Button } from '../../ui';
import { ItemList, type PinnedEntry } from './ItemList';
import { JsonPanel } from './JsonPanel';
import { SaveBar, type SaveStatus } from './SaveBar';
import type { DirtyReport, KindSpec } from './types';

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
  /** More on the left of the save bar, given the draft and its setter. */
  barExtra?: (draft: Draft<T>, setDraft: (next: Draft<T>) => void) => ReactNode;
  /** The kind lives in the active profile. A profile switch then loads
   *  the new profile's list even over unsaved changes, since saving
   *  them would write one profile's items into another. */
  profileScoped?: boolean;
}

const SAVED_MS = 2000;
const JSON_PARSE_MS = 150;

/** One kind's list, detail card, and save bar over a draft. The draft
 *  loads when the editor mounts. Save validates, writes it through the
 *  kind's API, and loads it again, so the page shows what the store
 *  kept. Discard puts the last load back, or the list as the store holds
 *  it now when it changed elsewhere while you edited. */
export function DraftEditor<T>({
  spec,
  json,
  onJson,
  onDirty,
  onError,
  pinned = null,
  pinnedSeq = 0,
  barExtra,
  profileScoped = false,
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
  const draftRef = useRef<Draft<T> | null>(null);
  const selectedRef = useRef<string | null>(null);
  const pinnedUid = pinned?.uid ?? null;
  const pinnedUidRef = useRef<string | null>(pinnedUid);
  const jsonTimer = useRef<number | undefined>(undefined);
  /** A save is writing. Store changes wait for the load that ends it. */
  const savingRef = useRef(false);
  /** The store changed while the draft held unsaved edits, so the last
   *  load no longer matches it. */
  const staleRef = useRef(false);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  /** Goes up after each delete, so focus moves once the list redraws. */
  const [deleteSeq, setDeleteSeq] = useState(0);
  const afterDeleteRef = useRef<string | null>(null);

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

  // Load and keep the selection on the same item by its natural key.
  const load = useCallback(
    async (keepKey: string | null) => {
      const values = await spec.load();
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
    spec
      .load()
      .then((values) => {
        if (cancelled) return;
        setDraft(createDraft(values));
        if (spec.json) setJsonText(spec.json.toText(values));
      })
      .catch((e) => {
        if (!cancelled) onError(automationSaveError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [spec, onError, setDraft]);

  // The store changed outside the page, like #trigger in the main
  // window or a script. Follow it while the draft is clean. With unsaved
  // changes, keep them and say so. Save applies them over the new list.
  useEffect(() => {
    if (!spec.subscribe) return;
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void spec
      .subscribe(() => {
        const d = draftRef.current;
        if (cancelled || !d) return;
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

  // A profile switch. A kind that lives in the profile loads the new
  // profile's list and says so when that drops unsaved changes. A kind
  // shared by every profile follows only while clean.
  const scopedRef = useRef(profileScoped);
  useEffect(() => {
    scopedRef.current = profileScoped;
  }, [profileScoped]);
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeProfileSwitched(() => {
      const d = draftRef.current;
      const dirty = d !== null && isDraftDirty(d);
      if (cancelled || (dirty && !scopedRef.current)) return;
      void load(dirty ? null : selectedKey())
        .then(() => {
          if (dirty) {
            onError(
              `Vosh switched profiles and loaded that profile's ${spec.noun.many}, so your unsaved changes are gone.`,
            );
          }
        })
        .catch((e) => onError(automationSaveError(e)));
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
  const allSections = useMemo(() => buildSections(entries), [entries]);
  const sections = useMemo(() => filterSections(allSections, filter), [allSections, filter]);
  const order = useMemo(
    () => [...(pinned ? [pinned.uid] : []), ...sectionOrder(sections)],
    [pinned, sections],
  );

  // Keep a selection: the first row when none is set or the selected
  // item is gone.
  useEffect(() => {
    if (!draft) return;
    const exists =
      selected !== null &&
      ((pinned !== null && selected === pinned.uid) || findDraftItem(draft, selected));
    if (!exists) setSelected(order[0] ?? null);
  }, [draft, selected, order, pinned]);

  // Select the pinned block once per request, as soon as it has loaded.
  const pinnedSeqDone = useRef(0);
  useEffect(() => {
    if (!pinnedSeq || pinnedUid === null || pinnedSeqDone.current === pinnedSeq) return;
    pinnedSeqDone.current = pinnedSeq;
    setSelected(pinnedUid);
  }, [pinnedSeq, pinnedUid]);

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

  const onNew = () => {
    const d = draftRef.current;
    if (!d || !spec.blank) return;
    const uid = nextDraftUid();
    setDraft(addDraftItem(d, spec.blank(), uid));
    setFilter('');
    setSelected(uid);
    setFresh(uid);
    setRevealSeq((n) => n + 1);
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
    target?.focus();
    row?.scrollIntoView({ block: 'nearest' });
  }, [deleteSeq]);

  // JSON edits reach the draft after a short pause. Take the text
  // still waiting now, or say it does not read.
  const jsonPending = useRef<string | null>(null);
  const applyJson = (text: string): boolean => {
    jsonPending.current = null;
    window.clearTimeout(jsonTimer.current);
    const values = spec.json?.fromText(text) ?? null;
    const d = draftRef.current;
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
    onError(null);
  };

  const onSave = async () => {
    if (jsonPending.current !== null && !applyJson(jsonPending.current)) return;
    const d = draftRef.current;
    if (!d || busy) return;
    const problem = spec.validate?.(draftValues(d)) ?? pinned?.validate?.() ?? null;
    if (problem) {
      onError(problem);
      return;
    }
    onError(null);
    setBusy(true);
    savingRef.current = true;
    try {
      await saveListThenPinned({
        list: isDraftDirty(d) ? () => spec.save(d) : null,
        pinned: pinned?.dirty ? () => pinned.save() : null,
        reload: () => load(selectedKey()),
      });
      setJustSaved(true);
    } catch (e) {
      onError(automationSaveError(e));
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
          revealInList: () => setRevealSeq((n) => n + 1),
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
              onFilter={setFilter}
              sections={sections}
              hasItems={(draft?.items.length ?? 0) > 0}
              emptyText={spec.emptyList}
              pinned={pinned}
              selected={selected}
              onSelect={onSelect}
              revealSeq={revealSeq}
              monoName={spec.monoName ?? false}
              monoMeta={spec.monoMeta ?? false}
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
        canSave={!(jsonOpen && jsonBad)}
        busy={busy || !draft}
        onDiscard={onDiscard}
        onSave={() => void onSave()}
      />
    </>
  );
}
