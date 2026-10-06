import { useEffect, useId, useRef, useState } from 'react';
import {
  createDraft,
  discardDraft,
  isDraftDirty,
  updateDraftItem,
  type Draft,
} from '../../automation/automationDraft';
import {
  automationSaveError,
  blankTimer,
  jsonListText,
  normalizeTick,
  normalizeTimer,
  parseJsonList,
  saveTimerDraft,
  timerEntry,
  timerKey,
  timerStore,
  validateTimers,
  type TimerRecord,
} from '../../automation/automationRecords';
import { withGroup } from '../../automation/automationTriggers';
import { subscribeTimersChanged, timersList } from '../../ipc/automation';
import { tickGetConfig, tickSetConfig, type TickConfig } from '../../ipc/tick';
import { followTickDraft } from '../../automation/tickDraft';
import { Card, Disclosure, Field, FieldArea, Row, Toggle } from '../../ui';
import { getShownProfile, isShownHeld, subscribeShownMoves } from '../shownProfile';
import { DraftEditor, type PinnedPart } from './DraftEditor';
import { GroupField, NumberField } from './fields';
import type { DetailProps, DirtyReport, KindSpec } from './types';

const TIMERS_SPEC: KindSpec<TimerRecord> = {
  id: 'timers',
  groups: 'timers',
  noun: { one: 'timer', many: 'timers' },
  filterLabel: 'Filter timers',
  newLabel: 'New timer',
  deleteLabel: 'Delete timer',
  emptyDetail: 'Choose a timer to edit it.',
  emptyList: 'You have no timers yet.',
  load: async (profile) => (await timersList(profile)).map(normalizeTimer),
  // One call per timer, the way the old Timers tab saved cards.
  save: (draft, written, profile) => saveTimerDraft(draft, written, timerStore(profile)),
  validate: validateTimers,
  entry: timerEntry,
  keyOf: timerKey,
  blank: blankTimer,
  json: {
    toText: jsonListText,
    fromText: (text) => parseJsonList(text, normalizeTimer),
  },
  subscribe: (onChange) => subscribeTimersChanged(() => onChange()),
  renderDetail: (props) => <TimerDetail {...props} />,
};

const TICK_UID = '\u0000tick';

interface TimersEditorProps {
  json: boolean;
  onJson: (open: boolean) => void;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
  /** Select the Tick each time this goes up. */
  tickSeq: number;
}

const loadTick = async (profile: string | undefined) =>
  createDraft([normalizeTick(await tickGetConfig(profile))]);

/** Timers, with the Tick pinned above them. The tick keeps its own
 *  draft, and the save bar covers both. */
export function TimersEditor({ json, onJson, onDirty, onError, tickSeq }: TimersEditorProps) {
  const [tick, setTick] = useState<Draft<TickConfig> | null>(null);
  const tickRef = useRef<Draft<TickConfig> | null>(null);

  const putTick = (next: Draft<TickConfig> | null) => {
    tickRef.current = next;
    setTick(next);
  };

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    // Only the newest read lands, so the tick of a profile Settings
    // left never shows.
    let reads = 0;
    const reload = (profile: string | undefined) => {
      const mine = ++reads;
      return loadTick(profile)
        .then((next) => {
          if (!cancelled && mine === reads) putTick(next);
        })
        .catch((e) => {
          if (!cancelled && mine === reads) onError(automationSaveError(e));
        });
    };
    void reload(getShownProfile());
    // The tick lives in the profile, so a switch, a load, a reset, an
    // import, or Settings moving to another profile reads the new one. A
    // replace speaks of the profile in front, which a login switch
    // replaces before the session list says the session moved, so that
    // read names no profile. A change from elsewhere lands while the tick
    // is clean and Settings shows the profile in front.
    void followTickDraft({
      reload: () => void reload(undefined),
      adopt: (cfg) => {
        if (!cancelled) putTick(createDraft([normalizeTick(cfg)]));
      },
      keeps: () => {
        const current = tickRef.current;
        return (current !== null && isDraftDirty(current)) || isShownHeld();
      },
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    const stopMoves = subscribeShownMoves((profile) => void reload(profile));
    return () => {
      cancelled = true;
      unsub?.();
      stopMoves();
    };
  }, [onError]);

  const tickItem = tick?.items[0];
  const pinned: PinnedPart | null =
    tick && tickItem
      ? {
          uid: TICK_UID,
          name: 'Tick',
          enabled: tickItem.value.enabled,
          anchor: 'tick',
          dirty: isDraftDirty(tick),
          phrase: 'the tick',
          save: async () => {
            const saved = await tickSetConfig(normalizeTick(tickItem.value), getShownProfile());
            putTick(createDraft([normalizeTick(saved)]));
          },
          discard: () => {
            const current = tickRef.current;
            if (current) putTick(discardDraft(current));
          },
          render: () => (
            <TickCard
              value={tickItem.value}
              update={(fn) => {
                const current = tickRef.current;
                const item = current?.items[0];
                if (current && item) putTick(updateDraftItem(current, item.uid, fn));
              }}
            />
          ),
        }
      : null;

  return (
    <DraftEditor
      spec={TIMERS_SPEC}
      json={json}
      onJson={onJson}
      onDirty={onDirty}
      onError={onError}
      pinned={pinned}
      pinnedSeq={tickSeq}
    />
  );
}

function TimerDetail({ value: t, update, fresh, revealInList }: DetailProps<TimerRecord>) {
  const nameRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (fresh) nameRef.current?.focus();
  }, [fresh]);

  const set = (patch: Partial<TimerRecord>) => update((v) => ({ ...v, ...patch }));

  return (
    <Card className="st-auto-card">
      <Row label="Name">
        <Field
          ref={nameRef}
          width="100%"
          value={t.name}
          placeholder="Optional"
          onChange={(name) => set({ name })}
        />
      </Row>
      <Row label="Group">
        <GroupField
          width="100%"
          value={t.group ?? ''}
          onCommit={(group) => {
            update((v) => withGroup(v, group));
            revealInList();
          }}
        />
      </Row>
      <Row label="Every">
        <NumberField
          value={t.interval_secs}
          min={1}
          max={86400}
          unit="seconds"
          onChange={(interval_secs) => set({ interval_secs })}
        />
      </Row>
      <Row label="Command">
        <FieldArea
          mono
          width="100%"
          value={t.command}
          placeholder="A line to send, or #lua"
          onChange={(command) => set({ command })}
        />
      </Row>
      <Row label="Enabled">
        <Toggle checked={t.enabled} onChange={(enabled) => set({ enabled })} />
      </Row>
    </Card>
  );
}

function TickCard({
  value: v,
  update,
}: {
  value: TickConfig;
  update: (fn: (value: TickConfig) => TickConfig) => void;
}) {
  const [advanced, setAdvanced] = useState(false);
  const advancedId = useId();
  const set = (patch: Partial<TickConfig>) => update((prev) => ({ ...prev, ...patch }));
  const text = (s: string) => (s.length > 0 ? s : null);
  const warnOn = v.warn_at_secs !== null && v.warn_at_secs > 0;

  return (
    <Card className="st-auto-card">
      <Row label="Enabled">
        <Toggle checked={v.enabled} onChange={(enabled) => set({ enabled })} />
      </Row>
      <Row label="Every">
        <NumberField
          value={v.interval_secs}
          min={1}
          max={3600}
          unit="seconds"
          onChange={(interval_secs) => set({ interval_secs })}
        />
      </Row>
      <Row label="Send each tick">
        <Field
          mono
          width="100%"
          value={v.auto_fire ?? ''}
          placeholder="No command"
          onChange={(s) => set({ auto_fire: text(s) })}
        />
      </Row>
      <Row label="Reset on">
        <Field
          mono
          width="100%"
          value={v.reset_pattern ?? ''}
          placeholder="A pattern that restarts it"
          onChange={(s) => set({ reset_pattern: text(s) })}
        />
      </Row>
      <Row label="Warn before it fires">
        <Toggle
          checked={warnOn}
          onChange={(on) => set({ warn_at_secs: on ? (v.warn_at_secs ?? 5) : null })}
        />
      </Row>
      <Row label="Warn at">
        <NumberField
          value={v.warn_at_secs ?? 5}
          min={1}
          max={300}
          disabled={!warnOn}
          unit="seconds left"
          onChange={(warn_at_secs) => set({ warn_at_secs })}
        />
      </Row>
      <Row label="Warning text">
        <Field
          mono
          width="100%"
          value={v.warn_message ?? ''}
          placeholder="Tick incoming"
          disabled={!warnOn}
          onChange={(s) => set({ warn_message: text(s) })}
        />
      </Row>
      <Row label="Warning color">
        <Field
          width="100%"
          value={v.warn_color ?? ''}
          placeholder="bright-red, #ff5555, or 196"
          disabled={!warnOn}
          onChange={(s) => set({ warn_color: text(s) })}
        />
      </Row>
      <Disclosure
        label="Advanced"
        description="Play a sound when the tick fires."
        expanded={advanced}
        aria-controls={advancedId}
        onClick={() => setAdvanced((open) => !open)}
      />
      {advanced && (
        <div id={advancedId} className="st-auto-advanced">
          <Row label="Play a sound">
            <Toggle checked={v.sound} onChange={(sound) => set({ sound })} />
          </Row>
        </div>
      )}
    </Card>
  );
}
