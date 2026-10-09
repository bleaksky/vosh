import { useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { withAlertPart } from '../../automation/alertParts';
import {
  draftChanges,
  isDraftDirty,
  type Draft,
  type DraftItem,
} from '../../automation/automationDraft';
import { groupKeyOf, searchText } from '../../automation/automationList';
import { jsonListText, parseJsonList } from '../../automation/automationRecords';
import {
  blankTrigger,
  effectOf,
  loadTriggers,
  normalizeTrigger,
  patternSource,
  replaceTemplateOf,
  saveTriggerDraft,
  TRIGGER_STYLE_OPTIONS,
  triggerKey,
  triggerStore,
  triggerStyle,
  validateTriggers,
  withEffect,
  withGroup,
  withReplaceTemplate,
  withTriggerStyle,
  type TriggerStyle,
} from '../../automation/automationTriggers';
import {
  subscribeTriggersChanged,
  type AlertParts,
  type TriggerAction,
  type TriggerRecord,
} from '../../ipc/automation';
import {
  Button,
  Card,
  CardNote,
  Disclosure,
  Field,
  FieldArea,
  Row,
  Select,
  Toggle,
} from '../../ui';
import {
  cardEdits,
  cardFlags,
  changedRows,
  editColors,
  libraryTrigger,
  NO_ROW,
  presetCard,
  shippedCard,
  withRow,
  type Rows,
  type TriggerCard,
} from '../../automation/presetEdits';
import { runPresetPlan } from '../../automation/presetPlan';
import { presetById } from '../../automation/presets';
import {
  presetEditsGet,
  presetEditsSet,
  type EditRow,
  type EditValue,
  type PresetEdits,
} from '../../ipc/presetEdits';
import { listJoin } from '../../lib/text';
import { usePromptGags } from '../../stores/session/promptGagStore';
import { AlertPartsOn, AlertRow } from './AlertRows';
import { useBannerPermission } from './useBannerPermission';
import { FixChoice, GroupField } from './fields';
import { DraftEditor } from './DraftEditor';
import {
  colorKeys,
  joinNodes,
  mainKeyOf,
  OpenPresetContext,
  opensAdvanced,
  presetSwitch,
  presetText,
  withAlert,
  type Flag,
  type FromPreset,
} from './triggerChanged';
import { AdvancedCount, Changed, UnderField } from './TriggerChangeNotes';
import { TriggerAdvanced } from './TriggerAdvanced';
import { PatternRow } from './TriggerPatterns';
import type { DetailProps, EditorProps, KindSpec, TriggersLink } from './types';

/** What the list loaded beside the triggers: your preset edits, and each
 *  preset trigger as the store holds it, by name. */
interface Loaded {
  edits: PresetEdits;
  stored: ReadonlyMap<string, TriggerRecord>;
}

/** `draft` as the trigger store takes it. A preset trigger goes back as
 *  the store holds it, in the group its card sets, since the store keeps
 *  the group the preset plan falls back to. */
function storeDraft(draft: Draft<TriggerCard>, { stored }: Loaded): Draft<TriggerRecord> {
  const toStore = (item: DraftItem<TriggerCard>): DraftItem<TriggerRecord> => {
    const copy = stored.get(item.value.name);
    if (!copy || !libraryTrigger(item.value)) return item;
    return { uid: item.uid, value: withGroup(copy, item.value.group ?? '') };
  };
  return { items: draft.items.map(toStore), saved: draft.saved.map(toStore) };
}

/** `t` as one of your triggers, with no preset tag. */
function asYours(t: TriggerRecord): TriggerRecord {
  const { preset: _preset, ...yours } = t;
  return yours;
}

/** The Triggers kind. Each preset trigger shows as its card, the
 *  library's trigger with your edits over it (presetCard). Save writes
 *  your triggers through the store, and the rows you changed in a preset
 *  trigger through preset_edits_set, then runs the preset plan for the
 *  profile, which builds and installs them. Edit all as JSON lists your
 *  triggers only, and keeps every preset trigger as it is. */
function triggersSpec(): KindSpec<TriggerCard> {
  let loaded: Loaded = { edits: {}, stored: new Map() };
  /** Your edits to the preset trigger `t` as they loaded. */
  const heldOf = (t: TriggerRecord) =>
    t.preset ? loaded.edits[t.preset]?.triggers?.[t.name] : undefined;
  return {
    id: 'triggers',
    groups: 'triggers',
    noun: { one: 'trigger', many: 'triggers' },
    filterLabel: 'Filter triggers',
    newLabel: 'New trigger',
    deleteLabel: 'Delete trigger',
    // Presets put their triggers back at launch. Turn the preset off.
    canDelete: (t) => !t.preset,
    emptyDetail: 'Choose a trigger to edit it.',
    emptyList: 'You have no triggers yet.',
    load: async (profile) => {
      const [list, edits] = await Promise.all([
        loadTriggers(triggerStore(profile)),
        presetEditsGet(profile),
      ]);
      loaded = {
        edits: edits ?? {},
        stored: new Map(list.filter((t) => t.preset).map((t) => [t.name, t])),
      };
      return list.map((t) => (t.preset ? presetCard(t, loaded.edits[t.preset]) : undefined) ?? t);
    },
    save: async (draft, _written, profile) => {
      const now = loaded;
      const list = storeDraft(draft, now);
      if (isDraftDirty(list)) await saveTriggerDraft(list, triggerStore(profile));
      const edits = cardEdits(draftChanges(draft).changed, now.edits);
      for (const [id, edit] of edits) await presetEditsSet(id, edit, profile);
      if (edits.size > 0) await runPresetPlan(profile ?? null);
    },
    validate: validateTriggers,
    entry: (t) => ({
      name: t.name,
      group: groupKeyOf(t.group),
      enabled: t.enabled,
      preset: Boolean(t.preset),
      edited: Boolean(heldOf(t)),
      ...fixWarn(t, heldOf(t)),
      // A preset fix notice opens Settings on the trigger it names.
      anchor: `triggers:${t.name}`,
      // A preset trigger also answers to its preset's name, which a link
      // from the preset's card fills the filter with.
      text: searchText(
        t.name,
        t.preset ? presetById(t.preset)?.name : undefined,
        t.group,
        t.patterns.map(patternSource).join('\n'),
        effectOf(t.actions, 'send'),
      ),
    }),
    keyOf: triggerKey,
    // A preset's card links to its triggers by name. A preset trigger's
    // name is its own, so the first of the name is the one.
    linkKeyOf: (t) => t.name,
    blank: blankTrigger,
    json: {
      toText: (values) => jsonListText(values.filter((t) => !t.preset)),
      fromText: (text, current) => {
        const yours = parseJsonList(text, normalizeTrigger);
        return yours && [...current.filter((t) => t.preset), ...yours.map(asYours)];
      },
    },
    subscribe: subscribeTriggersChanged,
    renderDetail: (props) => (
      <TriggerDetail
        {...props}
        swatches={props.value.preset ? editColors(loaded.edits[props.value.preset]) : undefined}
        held={heldOf(props.value)}
      />
    ),
  };
}

export function TriggersEditor({
  open = null,
  onOpenPreset = () => {},
  ...props
}: EditorProps & { open?: TriggersLink | null; onOpenPreset?: (id: string) => void }) {
  const [spec] = useState(triggersSpec);
  // A trigger that hid your prompt this session while the profile reads
  // no prompt carries the warn ring in the list.
  const gags = usePromptGags();
  const warnNotes = useMemo(
    () => new Map([...gags].map((name) => [name, HIDES_PROMPT_NOTE])),
    [gags],
  );
  const selectKey = useMemo(
    () => (open?.select ? { key: open.select, seq: open.seq } : null),
    [open],
  );
  const filterTo = useMemo(
    () => (open?.filter ? { text: open.filter, seq: open.seq } : null),
    [open],
  );
  return (
    <OpenPresetContext.Provider value={onOpenPreset}>
      <DraftEditor
        spec={spec}
        {...props}
        warnNotes={warnNotes}
        selectKey={selectKey}
        filterTo={filterTo}
      />
    </OpenPresetContext.Provider>
  );
}

/** Why a trigger carries the warn ring: it hid your prompt this session
 *  while the profile reads no prompt, so Vosh drew nothing in its
 *  place. */
export const HIDES_PROMPT_NOTE =
  "This trigger hides your prompt, and this profile draws nothing in its place. Turn it off, or tell Vosh your game's prompt in Customize prompt.";

/** Each row of a trigger's card by the key its edits keep, as a fix
 *  note names it. */
const ROW_LABELS: Readonly<Record<string, string>> = {
  enabled: 'Enabled',
  priority: 'Priority',
  group: 'Group',
  target: 'Match',
  mode: 'Pattern',
  style: 'Style',
  replace: 'Replace with',
  fg: 'Text color',
  bg: 'Background',
  bold: 'Bold',
  underline: 'Underline',
  inverse: 'Inverse',
  send: 'Then send',
  route: 'Send to pane',
  script: 'Lua script',
  alert: 'Alert',
};

/** The rows `flags` names, as a fix note lists them, each once. */
function flagLabels(ship: TriggerRecord, flags: Rows): string[] {
  const main = mainKeyOf(ship);
  const label = (key: string) =>
    key === main ? 'Pattern' : key.startsWith('pattern:') ? 'More patterns' : ROW_LABELS[key];
  return [...new Set(Object.keys(flags).map(label))].filter(Boolean);
}

/** What a fix note says after the preset's name. */
const fixTail = (labels: readonly string[]) =>
  `changed ${listJoin(labels)}, ${labels.length === 1 ? 'a row' : 'rows'} you edited.`;

/** The warn ring of a preset trigger a fix changed under your edit, with
 *  the note a reader hears, on or off. */
function fixWarn(
  t: TriggerCard,
  held: Readonly<Record<string, EditRow>> | undefined,
): { warn?: string } {
  const from = libraryTrigger(t);
  const flags = from ? cardFlags(t, held) : {};
  if (!from || Object.keys(flags).length === 0) return {};
  return {
    warn: `A fix to ${from.preset.name} ${fixTail(flagLabels(shippedCard(t), flags))}`,
  };
}

/** The card for the selected trigger. A preset trigger edits as yours
 *  do, all but its name, and each row you changed says what the preset
 *  has. `swatches` are your colors of its preset. */
export function TriggerDetail({
  value: t,
  update,
  fresh,
  revealInList,
  swatches,
  held,
}: DetailProps<TriggerCard> & {
  swatches?: Readonly<Record<string, string>> | undefined;
  /** Your edits to this preset trigger as they loaded, whose flagged
   *  rows carry Take the fix and Keep mine. */
  held?: Readonly<Record<string, EditRow>> | undefined;
}) {
  const flags = useMemo(() => cardFlags(t, held), [t, held]);
  const [advanced, setAdvanced] = useState(() => {
    const main = mainKeyOf(shippedCard(t));
    return Object.keys(flags).some((key) => opensAdvanced(key, main));
  });
  const advancedId = useId();
  const nameRef = useRef<HTMLInputElement | null>(null);
  const openPreset = useContext(OpenPresetContext);
  const from = useMemo((): FromPreset | null => {
    const lib = libraryTrigger(t);
    if (!lib) return null;
    const { colors } = lib.preset;
    return {
      preset: lib.preset,
      ship: shippedCard(t),
      changed: changedRows(t),
      colorOf: (key) => swatches?.[key] ?? colors[key]?.token ?? key,
    };
  }, [t, swatches]);
  // A preset trigger this build no longer builds stays as the store has
  // it until the preset plan takes it out.
  const locked = Boolean(t.preset) && !from;
  const style = triggerStyle(t.actions);
  const gags = usePromptGags();
  const hidesPrompt = t.enabled && gags.has(t.name);
  const banner = useBannerPermission();

  useEffect(() => {
    if (fresh) nameRef.current?.focus();
  }, [fresh]);

  const set = (patch: Partial<TriggerRecord>) => update((v) => ({ ...v, ...patch }));
  const setActions = (fn: (actions: TriggerAction[]) => TriggerAction[]) =>
    update((v) => ({ ...v, actions: fn(v.actions) }));
  const flag: Flag = (key, say) => {
    if (!Object.hasOwn(flags, key)) return null;
    const now = flags[key];
    const sends = key === 'send';
    return (
      <FixChoice
        verb={sends ? 'sends' : 'has'}
        onTake={() => update((v) => withRow(v, key, now))}
        onKeep={() => update((v) => ({ ...v, kept: [...(v.kept ?? []), key] }))}
      >
        {sends && now === NO_ROW ? 'nothing' : say(now)}
      </FixChoice>
    );
  };
  const changed = (key: string, say: (value: EditValue) => ReactNode): ReactNode =>
    flag(key, say) ??
    (from && Object.hasOwn(from.changed, key) ? <Changed>{say(from.changed[key])}</Changed> : null);
  const fixLabels = from ? flagLabels(from.ship, flags) : [];
  const replaceKeys = from ? colorKeys(from.preset, replaceTemplateOf(from.ship.actions)) : [];

  return (
    <>
      <Card className="st-auto-card">
        {hidesPrompt && <CardNote tone="warn">{HIDES_PROMPT_NOTE}</CardNote>}
        {from && fixLabels.length > 0 && (
          <CardNote tone="warn">
            A fix to{' '}
            <button
              type="button"
              className="st-auto-link"
              onClick={() => openPreset(from.preset.id)}
            >
              {from.preset.name}
            </button>{' '}
            {fixTail(fixLabels)}
          </CardNote>
        )}
        {from && fixLabels.length === 0 && (
          <CardNote>
            From{' '}
            <button
              type="button"
              className="st-auto-link"
              onClick={() => openPreset(from.preset.id)}
            >
              {from.preset.name}
            </button>
            . The rows you change here stay yours, and the fixes Vosh ships for the preset still
            reach the rest.
          </CardNote>
        )}
        {locked && (
          <CardNote>
            This trigger comes from a preset, so only its group changes here. Turn the preset off
            under Presets to remove it.
          </CardNote>
        )}
        <Row label="Name">
          <Field
            ref={nameRef}
            width="100%"
            value={t.name}
            disabled={Boolean(t.preset)}
            onChange={(name) => set({ name })}
          />
        </Row>
        <Row label="Group">
          <UnderField changed={changed('group', (g) => (g ? presetText(g) : 'no group'))}>
            <GroupField
              width="100%"
              value={t.group ?? ''}
              onCommit={(group) => {
                update((v) => withGroup(v, group));
                revealInList();
              }}
            />
          </UnderField>
        </Row>
        <PatternRow t={t} update={update} locked={locked} ship={from?.ship ?? null} flag={flag} />
        <Row label="Style" description={changed('style', styleLabel)}>
          <Select
            width="100%"
            value={style}
            options={TRIGGER_STYLE_OPTIONS}
            disabled={locked}
            onChange={(next) => setActions((a) => withTriggerStyle(a, next as TriggerStyle))}
          />
        </Row>
        {style === 'replace' && (
          <Row
            label="Replace with"
            description={
              replaceKeys.length > 0 ? (
                <>
                  {joinNodes(
                    replaceKeys.map((key) => (
                      <span key={key} className="st-auto-mono">{`{${key}}`}</span>
                    )),
                  )}{' '}
                  {replaceKeys.length === 1 ? 'follows' : 'follow'} the preset’s card
                </>
              ) : undefined
            }
          >
            <UnderField changed={changed('replace', presetText)}>
              <FieldArea
                mono
                width="100%"
                value={replaceTemplateOf(t.actions)}
                disabled={locked}
                onChange={(template) => setActions((a) => withReplaceTemplate(a, template))}
              />
            </UnderField>
          </Row>
        )}
        <Row label="Then send">
          <UnderField changed={changed('send', presetText)}>
            <FieldArea
              mono
              width="100%"
              value={effectOf(t.actions, 'send')}
              disabled={locked}
              onChange={(command) => setActions((a) => withEffect(a, 'send', command))}
            />
          </UnderField>
        </Row>
        <AlertRow
          alert={t.alert}
          disabled={locked}
          banner={banner}
          description={
            flag('alert', (v) => (
              <AlertPartsOn
                alert={v === NO_ROW ? undefined : (v as unknown as AlertParts)}
                none="no alert"
              />
            )) ??
            (from && partsOn(t.alert) !== partsOn(from.ship.alert) ? (
              <Changed>
                <AlertPartsOn alert={from.ship.alert} none="no alert" />
              </Changed>
            ) : undefined)
          }
          onPress={(part, on) => update((v) => withAlert(v, (a) => withAlertPart(a, part, on)))}
        />
        <Row label="Enabled" description={changed('enabled', presetSwitch)}>
          <Toggle checked={t.enabled} disabled={locked} onChange={(enabled) => set({ enabled })} />
        </Row>
        <Disclosure
          label="Advanced"
          description="Set priority, match prompts, send to a pane, run Lua, or tune alerts."
          note={<AdvancedCount t={t} from={from} />}
          expanded={advanced}
          aria-controls={advancedId}
          onClick={() => setAdvanced((open) => !open)}
        />
        {advanced && (
          <div id={advancedId} className="st-auto-advanced">
            <TriggerAdvanced
              t={t}
              update={update}
              locked={locked}
              style={style}
              from={from}
              changed={changed}
              flag={flag}
            />
          </div>
        )}
      </Card>
      {from && (
        <div className="st-auto-detail-actions">
          <Button
            disabled={Object.keys(from.changed).length === 0}
            onClick={() => update(shippedCard)}
          >
            Reset to preset
          </Button>
        </div>
      )}
    </>
  );
}

const styleLabel = (value: EditValue) =>
  TRIGGER_STYLE_OPTIONS.find((o) => o.value === value)?.label ?? String(value);

/** Which parts an alert has on, to tell whether the Alert row
 *  changed. */
const partsOn = (alert: AlertParts | undefined) =>
  [Boolean(alert?.banner), alert?.sound !== undefined, alert?.attention !== undefined].join();
