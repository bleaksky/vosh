import {
  createContext,
  Fragment,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { alertOrNone, withAlertPart, withAlertParts } from '../../automation/alertParts';
import {
  countPhrase,
  draftChanges,
  isDraftDirty,
  type Draft,
  type DraftItem,
} from '../../automation/automationDraft';
import { groupKeyOf, searchText } from '../../automation/automationList';
import { jsonListText, parseJsonList } from '../../automation/automationRecords';
import {
  blankPattern,
  blankTrigger,
  effectOf,
  extraEffects,
  HIGHLIGHT_COLORS,
  highlightOf,
  loadTriggers,
  mainPattern,
  MATCH_MODE_DESCRIPTIONS,
  MATCH_MODE_OPTIONS,
  normalizeTrigger,
  patternSource,
  replaceTemplateOf,
  saveTriggerDraft,
  TRIGGER_STYLE_OPTIONS,
  triggerKey,
  triggerMode,
  triggerStore,
  triggerStyle,
  validateTriggers,
  withEffect,
  withEffectAt,
  withGroup,
  withHighlight,
  withMainPattern,
  withMainPatternSource,
  withPatternSource,
  withReplaceTemplate,
  withTriggerMode,
  withTriggerStyle,
  type HighlightPatch,
  type TriggerStyle,
} from '../../automation/automationTriggers';
import {
  subscribeTriggersChanged,
  type AlertParts,
  type HighlightStyle,
  type NamedColor,
  type TriggerAction,
  type TriggerPattern,
  type TriggerRecord,
} from '../../ipc/automation';
import {
  Button,
  Card,
  CardNote,
  ChipButton,
  CloseIcon,
  Disclosure,
  Field,
  FieldArea,
  PencilIcon,
  PlusIcon,
  Row,
  Segmented,
  Select,
  Toggle,
  type SelectOption,
} from '../../ui';
import {
  cardEdits,
  changedRows,
  editColors,
  libraryTrigger,
  presetCard,
  shippedCard,
  type Rows,
} from '../../automation/presetEdits';
import { runPresetPlan } from '../../automation/presetPlan';
import { presetById, type Preset } from '../../automation/presets';
import {
  presetEditsGet,
  presetEditsSet,
  type EditValue,
  type PresetEdits,
} from '../../ipc/presetEdits';
import { usePromptGags } from '../../stores/session/promptGagStore';
import { AlertDetailRows, AlertPartsOn, AlertRow } from './AlertRows';
import { useBannerPermission } from './useBannerPermission';
import { CodeRow, GroupField, NumberField } from './fields';
import { DraftEditor } from './DraftEditor';
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
function storeDraft(draft: Draft<TriggerRecord>, { stored }: Loaded): Draft<TriggerRecord> {
  const toStore = (item: DraftItem<TriggerRecord>): DraftItem<TriggerRecord> => {
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
 *  profile, which builds and installs them (Presets board 2). Edit all
 *  as JSON lists your triggers only, and keeps every preset trigger as
 *  it is (Q9). */
function triggersSpec(): KindSpec<TriggerRecord> {
  let loaded: Loaded = { edits: {}, stored: new Map() };
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
      edited: Boolean(t.preset && loaded.edits[t.preset]?.triggers?.[t.name]),
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
 *  while the profile reads no prompt, so Vosh drew nothing in its place
 *  (section 7 step 13 of the prompt build spec). */
export const HIDES_PROMPT_NOTE =
  "This trigger hides your prompt, and this profile draws nothing in its place. Turn it off, or tell Vosh your game's prompt in Customize prompt.";

const COLOR_OPTIONS: readonly SelectOption[] = [
  { value: '', label: 'Default' },
  ...HIGHLIGHT_COLORS,
];

const MATCH_OPTIONS = [
  { value: 'line', label: 'Lines' },
  { value: 'prompt', label: 'Prompts' },
  { value: 'room', label: 'Room' },
  { value: 'room_target', label: 'Your target' },
] as const;

const EFFECT_LABELS = {
  send: 'Also send',
  route: 'Also send to pane',
  script: 'Also run Lua',
} as const;

/** `v` with its alert table set by `fn`, or with none while the table
 *  is the default, so a trigger that rings nothing saves no alert. */
function withAlert(
  v: TriggerRecord,
  fn: (alert: AlertParts | undefined) => AlertParts,
): TriggerRecord {
  const next = { ...v };
  const alert = alertOrNone(fn(v.alert));
  if (alert) next.alert = alert;
  else delete next.alert;
  return next;
}

/** The color options, plus the stored one when it is not a named
 *  color, so a select never shows a value it does not hold. */
function colorOptions(current: string | undefined): readonly SelectOption[] {
  if (!current || COLOR_OPTIONS.some((o) => o.value === current)) return COLOR_OPTIONS;
  return [...COLOR_OPTIONS, { value: current, label: current }];
}

/** Opens a preset's card in Presets, by id, from the note at the head
 *  of a preset trigger's card. */
const OpenPresetContext = createContext<(id: string) => void>(() => {});

const PATTERN_NOUN = { one: 'pattern', many: 'patterns' };
const CHANGE_NOUN = { one: 'change', many: 'changes' };

/** The rows under Advanced, by the key their edits keep. */
const ADVANCED_ROWS = [
  'priority',
  'target',
  'fg',
  'bg',
  'bold',
  'underline',
  'inverse',
  'route',
  'script',
] as const;

/** How a changed row says what the preset has. */
function Changed({ children }: { children: ReactNode }) {
  return <span className="st-auto-changed">Changed. The preset has {children}.</span>;
}

/** The preset's text, in the MUD font, or that it leaves it empty. */
function presetText(value: EditValue | undefined): ReactNode {
  const text = typeof value === 'string' ? value : '';
  return text ? <span className="st-auto-mono">{text}</span> : 'it empty';
}

const presetSwitch = (value: EditValue | undefined) => (value === true ? 'it on' : 'it off');

/** A text row's field with the line under it that says what the preset
 *  has, once you changed it. */
function UnderField({ changed, children }: { changed: ReactNode; children: ReactNode }) {
  if (changed === null) return children;
  return (
    <div className="st-auto-stack">
      {children}
      <p className="st-auto-under">{changed}</p>
    </div>
  );
}

/** `description` with the line of a changed row after it, or either one
 *  alone. */
function withChanged(description: ReactNode, changed: ReactNode): ReactNode {
  if (changed === null) return description;
  return (
    <>
      {description}
      {changed}
    </>
  );
}

/** What a preset trigger's card knows of its preset: the trigger as
 *  the preset ships it, each row you changed with the preset's value,
 *  and the color your swatch or the preset gives each key. */
interface FromPreset {
  preset: Preset;
  ship: TriggerRecord;
  changed: Rows;
  colorOf: (key: string) => string;
}

/** The card for the selected trigger. A preset trigger edits as yours
 *  do, all but its name, and each row you changed says what the preset
 *  has (Presets board 2). `swatches` are your colors of its preset. */
export function TriggerDetail({
  value: t,
  update,
  fresh,
  revealInList,
  swatches,
}: DetailProps<TriggerRecord> & { swatches?: Readonly<Record<string, string>> | undefined }) {
  const [advanced, setAdvanced] = useState(false);
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
  const changed = (key: string, say: (value: EditValue) => ReactNode): ReactNode =>
    from && Object.hasOwn(from.changed, key) ? <Changed>{say(from.changed[key])}</Changed> : null;
  const replaceKeys = from ? colorKeys(from.preset, replaceTemplateOf(from.ship.actions)) : [];

  return (
    <>
      <Card className="st-auto-card">
        {hidesPrompt && <CardNote tone="warn">{HIDES_PROMPT_NOTE}</CardNote>}
        {from && (
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
        <PatternRow t={t} update={update} locked={locked} ship={from?.ship ?? null} />
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
            from && partsOn(t.alert) !== partsOn(from.ship.alert) ? (
              <Changed>
                <AlertPartsOn alert={from.ship.alert} none="no alert" />
              </Changed>
            ) : undefined
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

/** The color keys of `preset` that `template` names, in its order. */
function colorKeys(preset: Preset, template: string): string[] {
  const keys = [...template.matchAll(/\{([a-z_]+)\}/g)].map((m) => m[1]);
  return [...new Set(keys)].filter((key) => Object.hasOwn(preset.colors, key));
}

/** `parts` joined as a sentence joins them, `a and b`, `a, b, and c`. */
function joinNodes(parts: readonly ReactNode[]): ReactNode {
  return parts.map((part, i) => (
    <Fragment key={i}>
      {i === 0 ? '' : i === parts.length - 1 ? (parts.length > 2 ? ', and ' : ' and ') : ', '}
      {part}
    </Fragment>
  ));
}

const styleLabel = (value: EditValue) =>
  TRIGGER_STYLE_OPTIONS.find((o) => o.value === value)?.label ?? String(value);

/** Which parts an alert has on, to tell whether the Alert row
 *  changed. */
const partsOn = (alert: AlertParts | undefined) =>
  [Boolean(alert?.banner), alert?.sound !== undefined, alert?.attention !== undefined].join();

/** The rows under Advanced that differ from the preset, the count a
 *  closed Advanced shows beside the pencil so no edit hides there. */
function AdvancedCount({ t, from }: { t: TriggerRecord; from: FromPreset | null }) {
  if (!from) return null;
  const rows = ADVANCED_ROWS.filter((key) => Object.hasOwn(from.changed, key)).length;
  const count = rows + (morePatternsDiffer(t, from.ship) ? 1 : 0) + alsoChanges(t, from.ship);
  if (count === 0) return null;
  return (
    <span className="st-auto-count">
      <PencilIcon size={12} />
      {countPhrase(count, CHANGE_NOUN)}
    </span>
  );
}

/** Each pattern past the main one, and whether the main one is on. */
function morePatterns(t: TriggerRecord): string {
  return JSON.stringify([
    mainPattern(t).enabled,
    ...t.patterns.slice(1).map((p) => [patternSource(p), p.enabled]),
  ]);
}

const morePatternsDiffer = (t: TriggerRecord, ship: TriggerRecord) =>
  morePatterns(t) !== morePatterns(ship);

/** The commands of each Also send. */
const alsoSends = (t: TriggerRecord) =>
  extraEffects(t.actions)
    .filter((e) => e.kind === 'send')
    .map((e) => e.value);

/** How many Also send rows differ from the preset's: the ones you
 *  changed or added, or the ones you took out where those are more. */
function alsoChanges(t: TriggerRecord, ship: TriggerRecord): number {
  const mine = alsoSends(t);
  const theirs = alsoSends(ship);
  const added = mine.filter((c) => !theirs.includes(c)).length;
  const gone = theirs.filter((c) => !mine.includes(c)).length;
  return Math.max(added, gone);
}

/** The Pattern row as board 6 draws it: the label and what the mode
 *  does on the left with the mode beside them, and the main pattern at
 *  full width under both. Built by hand like CodeRow, since a Row keeps
 *  its control on the right of the label. */
function PatternRow({
  t,
  update,
  locked,
  ship,
}: {
  t: TriggerRecord;
  update: DetailProps<TriggerRecord>['update'];
  locked: boolean;
  ship: TriggerRecord | null;
}) {
  const labelId = useId();
  const descId = useId();
  const mode = triggerMode(t);
  const source = patternSource(mainPattern(t));
  let changed: ReactNode = null;
  if (ship && patternSource(mainPattern(ship)) !== source) {
    changed = <Changed>{presetText(patternSource(mainPattern(ship)))}</Changed>;
  } else if (ship && triggerMode(ship) !== mode) {
    const label = MATCH_MODE_OPTIONS.find((o) => o.value === triggerMode(ship))?.label;
    changed = <Changed>{label}</Changed>;
  }
  return (
    <div className="st-row st-auto-block">
      <div className="st-auto-block-head">
        <div className="st-row-text">
          <span id={labelId} className="st-row-label">
            Pattern
          </span>
          <span id={descId} className="st-row-desc">
            {MATCH_MODE_DESCRIPTIONS[mode]}
          </span>
        </div>
        <div className="st-row-control">
          <Segmented
            label="Match the pattern as"
            options={MATCH_MODE_OPTIONS.map((o) => ({ ...o, disabled: locked }))}
            value={mode}
            onChange={(next) => update((v) => withTriggerMode(v, next))}
          />
        </div>
      </div>
      <Field
        mono
        width="100%"
        aria-labelledby={labelId}
        aria-describedby={descId}
        value={source}
        disabled={locked}
        onChange={(pattern) => update((v) => withMainPatternSource(v, pattern))}
      />
      {changed && <p className="st-auto-under">{changed}</p>}
    </div>
  );
}

function TriggerAdvanced({
  t,
  update,
  locked,
  style,
  from,
  changed,
}: {
  t: TriggerRecord;
  update: DetailProps<TriggerRecord>['update'];
  locked: boolean;
  style: TriggerStyle;
  from: FromPreset | null;
  changed: (key: string, say: (value: EditValue) => ReactNode) => ReactNode;
}) {
  const setActions = (fn: (actions: TriggerAction[]) => TriggerAction[]) =>
    update((v) => ({ ...v, actions: fn(v.actions) }));
  const setHighlight = <K extends keyof HighlightStyle>(key: K, value: HighlightStyle[K]) =>
    setActions((a) => withHighlight(a, { [key]: value } as HighlightPatch));
  const highlight = highlightOf(t.actions);
  const extras = extraEffects(t.actions);
  const shipSends = from ? alsoSends(from.ship) : [];
  // A text color the preset names by key shows as the color it paints,
  // and picking that color again puts the key back, so the swatch on
  // the preset's card still reaches it.
  const shipFg = from ? highlightOf(from.ship.actions)?.fg : undefined;
  const isKey = (c: string | undefined): c is string =>
    c !== undefined && from !== null && Object.hasOwn(from.preset.colors, c);
  const shown = (c: string | undefined) => (isKey(c) ? from!.colorOf(c) : c);
  const colorWords = (value: EditValue) => {
    const c = typeof value === 'string' ? (/^\{(.+)\}$/.exec(value)?.[1] ?? value) : '';
    const named = shown(c) ?? '';
    return COLOR_OPTIONS.find((o) => o.value === named)?.label ?? named;
  };
  const shownFg = shown(highlight?.fg);

  return (
    <>
      <Row
        label="Priority"
        description={withChanged(
          'Vosh runs triggers with higher numbers first.',
          changed('priority', String),
        )}
      >
        <NumberField
          value={t.priority}
          min={0}
          max={99}
          disabled={locked}
          onChange={(priority) => update((v) => ({ ...v, priority }))}
        />
      </Row>
      <Row
        label="Match"
        description={withChanged(
          'Prompts match what your MUD sends before you type. Room matches the armies, things and people a room lists after its exits. Your target matches the line of the one you target with tar when a room lists them.',
          changed('target', (v) => MATCH_OPTIONS.find((o) => o.value === v)?.label ?? String(v)),
        )}
      >
        <Segmented
          options={MATCH_OPTIONS.map((o) => ({ ...o, disabled: locked }))}
          value={t.target ?? 'line'}
          onChange={(target) =>
            update((v) => {
              const next = { ...v };
              if (target === 'line') delete next.target;
              else next.target = target;
              return next;
            })
          }
        />
      </Row>
      <PatternsBlock t={t} update={update} locked={locked} ship={from?.ship ?? null} />
      {highlight && (style === 'highlight' || style === 'wash') && (
        <>
          <Row label="Text color" description={changed('fg', colorWords)}>
            <Select
              width={160}
              value={shownFg ?? ''}
              options={colorOptions(shownFg)}
              disabled={locked}
              onChange={(fg) =>
                setHighlight(
                  'fg',
                  (isKey(shipFg) && fg === shown(shipFg) ? shipFg : fg || undefined) as
                    | NamedColor
                    | undefined,
                )
              }
            />
          </Row>
          <Row label="Background" description={changed('bg', colorWords)}>
            <Select
              width={160}
              value={highlight.bg ?? ''}
              options={colorOptions(highlight.bg)}
              disabled={locked}
              onChange={(bg) => setHighlight('bg', (bg || undefined) as NamedColor | undefined)}
            />
          </Row>
          <Row label="Bold" description={changed('bold', presetSwitch)}>
            <Toggle
              checked={Boolean(highlight.bold)}
              disabled={locked}
              onChange={(on) => setHighlight('bold', on)}
            />
          </Row>
          <Row label="Underline" description={changed('underline', presetSwitch)}>
            <Toggle
              checked={Boolean(highlight.underline)}
              disabled={locked}
              onChange={(on) => setHighlight('underline', on)}
            />
          </Row>
          <Row label="Inverse" description={changed('inverse', presetSwitch)}>
            <Toggle
              checked={Boolean(highlight.inverse)}
              disabled={locked}
              onChange={(on) => setHighlight('inverse', on)}
            />
          </Row>
        </>
      )}
      <Row label="Send to pane" description="The pane that also gets the matching line, like chat.">
        <UnderField changed={changed('route', presetText)}>
          <Field
            width="100%"
            value={effectOf(t.actions, 'route')}
            placeholder="No pane"
            disabled={locked}
            onChange={(pane) => setActions((a) => withEffect(a, 'route', pane))}
          />
        </UnderField>
      </Row>
      <CodeRow
        label="Lua script"
        description={withChanged(
          'Runs on each match. The captures table holds what the pattern caught.',
          changed('script', (v) => (v ? 'a script of its own' : 'it empty')),
        )}
        value={effectOf(t.actions, 'script')}
        readOnly={locked}
        onChange={(body) => setActions((a) => withEffect(a, 'script', body))}
      />
      {extras.map((extra, i) => {
        const theirs = shipSends[extras.slice(0, i).filter((e) => e.kind === 'send').length];
        const mine = extra.kind === 'send' && from !== null && !shipSends.includes(extra.value);
        return (
          <Row key={extra.index} label={EFFECT_LABELS[extra.kind]}>
            <UnderField changed={mine ? <Changed>{presetText(theirs)}</Changed> : null}>
              <FieldArea
                className="st-auto-grow"
                mono={extra.kind !== 'route'}
                width="100%"
                value={extra.value}
                disabled={locked}
                onChange={(next) => setActions((a) => withEffectAt(a, extra.index, next))}
              />
            </UnderField>
            {!locked && (
              <button
                type="button"
                className="st-auto-iconbutton"
                aria-label={`Remove ${EFFECT_LABELS[extra.kind].toLowerCase()}`}
                onClick={() => setActions((a) => withEffectAt(a, extra.index, null))}
              >
                <CloseIcon size={12} />
              </button>
            )}
          </Row>
        );
      })}
      <AlertDetailRows
        alert={t.alert}
        disabled={locked}
        onChange={(patch) => update((v) => withAlert(v, (a) => withAlertParts(a, patch)))}
      />
    </>
  );
}

/** The patterns past the main one. Each has its own switch, and the
 *  trigger fires when any pattern that is on matches. */
function PatternsBlock({
  t,
  update,
  locked,
  ship,
}: {
  t: TriggerRecord;
  update: DetailProps<TriggerRecord>['update'];
  locked: boolean;
  ship: TriggerRecord | null;
}) {
  const headingId = useId();
  const main = mainPattern(t);
  const extra = t.patterns.slice(1);
  const setPatterns = (fn: (rest: TriggerPattern[], v: TriggerRecord) => TriggerPattern[]) =>
    update((v) => {
      const first = v.patterns[0] ?? mainPattern(v);
      return { ...v, patterns: [first, ...fn(v.patterns.slice(1), v)] };
    });

  return (
    <div className="st-row st-auto-block" role="group" aria-labelledby={headingId}>
      <div className="st-row-text">
        <span id={headingId} className="st-row-label">
          More patterns
        </span>
        <span className="st-row-desc">
          {withChanged(
            'The trigger fires when any pattern that is on matches.',
            ship && morePatternsDiffer(t, ship) ? (
              <Changed>{countPhrase(ship.patterns.length, PATTERN_NOUN)}</Changed>
            ) : null,
          )}
        </span>
      </div>
      <ul className="st-auto-patterns">
        <li className="st-auto-pattern">
          <Toggle
            aria-label="Use the main pattern"
            checked={main.enabled}
            disabled={locked}
            onChange={(enabled) => update((v) => withMainPattern(v, { enabled }))}
          />
          <span className="st-auto-pattern-main st-auto-mono">
            {patternSource(main) || 'Main pattern'}
          </span>
        </li>
        {extra.map((p, i) => (
          <li key={i} className="st-auto-pattern">
            <Toggle
              aria-label={`Use pattern ${i + 2}`}
              checked={p.enabled}
              disabled={locked}
              onChange={(enabled) =>
                setPatterns((rest) => rest.map((r, j) => (j === i ? { ...r, enabled } : r)))
              }
            />
            <Field
              mono
              width="100%"
              aria-label={`Pattern ${i + 2}`}
              value={patternSource(p)}
              disabled={locked}
              onChange={(pattern) =>
                setPatterns((rest) =>
                  rest.map((r, j) => (j === i ? withPatternSource(r, pattern) : r)),
                )
              }
            />
            {!locked && (
              <button
                type="button"
                className="st-auto-iconbutton"
                aria-label={`Remove pattern ${i + 2}`}
                onClick={() => setPatterns((rest) => rest.filter((_, j) => j !== i))}
              >
                <CloseIcon size={12} />
              </button>
            )}
          </li>
        ))}
      </ul>
      {!locked && (
        <div>
          <ChipButton
            icon={<PlusIcon size={12} />}
            onClick={() => setPatterns((rest, v) => [...rest, blankPattern(triggerMode(v))])}
          >
            Add pattern
          </ChipButton>
        </div>
      )}
    </div>
  );
}
