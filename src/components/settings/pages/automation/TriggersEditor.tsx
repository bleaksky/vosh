import { useEffect, useId, useRef, useState } from 'react';
import { groupKeyOf, searchText } from '../../../../automation/automationList';
import { jsonListText, parseJsonList } from '../../../../automation/automationRecords';
import {
  blankTrigger,
  effectOf,
  extraEffects,
  HIGHLIGHT_COLORS,
  highlightOf,
  loadTriggers,
  mainPattern,
  normalizeTrigger,
  patternSource,
  replaceTemplateOf,
  saveTriggerDraft,
  TRIGGER_STYLE_OPTIONS,
  triggerKey,
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
  withTriggerStyle,
  type HighlightPatch,
  type TriggerStyle,
} from '../../../../automation/automationTriggers';
import {
  subscribeTriggersChanged,
  type HighlightStyle,
  type NamedColor,
  type TriggerAction,
  type TriggerPattern,
  type TriggerRecord,
} from '../../../../ipc/automation';
import {
  Card,
  ChipButton,
  CloseIcon,
  Disclosure,
  Field,
  FieldArea,
  PlusIcon,
  Row,
  Segmented,
  Select,
  Toggle,
  type SelectOption,
} from '../../ui';
import { usePromptGags } from '../../../../stores/session/promptGagStore';
import { CardNote, CodeRow, GroupField, NumberField } from './fields';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, EditorProps, KindSpec } from './types';

const TRIGGERS_SPEC: KindSpec<TriggerRecord> = {
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
  load: () => loadTriggers(),
  save: (draft) => saveTriggerDraft(draft),
  validate: validateTriggers,
  entry: (t) => ({
    name: t.name,
    group: groupKeyOf(t.group),
    enabled: t.enabled,
    preset: Boolean(t.preset),
    text: searchText(
      t.name,
      t.group,
      t.patterns.map(patternSource).join('\n'),
      effectOf(t.actions, 'send'),
    ),
  }),
  keyOf: triggerKey,
  blank: blankTrigger,
  json: {
    toText: jsonListText,
    fromText: (text) => parseJsonList(text, normalizeTrigger),
  },
  subscribe: subscribeTriggersChanged,
  renderDetail: (props) => <TriggerDetail {...props} />,
};

export function TriggersEditor(props: EditorProps) {
  // A trigger that hid your prompt this session while the profile reads
  // no prompt carries the warn ring in the list.
  const gags = usePromptGags();
  return (
    <DraftEditor spec={TRIGGERS_SPEC} {...props} warnNames={gags} warnNote={HIDES_PROMPT_NOTE} />
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

/** The color options, plus the stored one when it is not a named
 *  color, so a select never shows a value it does not hold. */
function colorOptions(current: string | undefined): readonly SelectOption[] {
  if (!current || COLOR_OPTIONS.some((o) => o.value === current)) return COLOR_OPTIONS;
  return [...COLOR_OPTIONS, { value: current, label: current }];
}

function TriggerDetail({ value: t, update, fresh, revealInList }: DetailProps<TriggerRecord>) {
  const [advanced, setAdvanced] = useState(false);
  const advancedId = useId();
  const nameRef = useRef<HTMLInputElement | null>(null);
  const locked = Boolean(t.preset);
  const style = triggerStyle(t.actions);
  const gags = usePromptGags();
  const hidesPrompt = t.enabled && gags.has(t.name);

  useEffect(() => {
    if (fresh) nameRef.current?.focus();
  }, [fresh]);

  const set = (patch: Partial<TriggerRecord>) => update((v) => ({ ...v, ...patch }));
  const setActions = (fn: (actions: TriggerAction[]) => TriggerAction[]) =>
    update((v) => ({ ...v, actions: fn(v.actions) }));

  return (
    <Card className="st-auto-card">
      {hidesPrompt && (
        <p className="st-auto-cardnote is-warn">
          <span className="st-auto-warndot" aria-hidden="true" />
          <span>{HIDES_PROMPT_NOTE}</span>
        </p>
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
          disabled={locked}
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
      <Row label="Pattern">
        <Field
          mono
          width="100%"
          value={patternSource(mainPattern(t))}
          disabled={locked}
          onChange={(pattern) => update((v) => withMainPatternSource(v, pattern))}
        />
      </Row>
      <Row label="Style">
        <Select
          width="100%"
          value={style}
          options={TRIGGER_STYLE_OPTIONS}
          disabled={locked}
          onChange={(next) => setActions((a) => withTriggerStyle(a, next as TriggerStyle))}
        />
      </Row>
      {style === 'replace' && (
        <Row label="Replace with">
          <FieldArea
            mono
            width="100%"
            value={replaceTemplateOf(t.actions)}
            disabled={locked}
            onChange={(template) => setActions((a) => withReplaceTemplate(a, template))}
          />
        </Row>
      )}
      <Row label="Then send">
        <FieldArea
          mono
          width="100%"
          value={effectOf(t.actions, 'send')}
          disabled={locked}
          onChange={(command) => setActions((a) => withEffect(a, 'send', command))}
        />
      </Row>
      <Row label="Enabled">
        <Toggle checked={t.enabled} disabled={locked} onChange={(enabled) => set({ enabled })} />
      </Row>
      <Disclosure
        label="Advanced"
        description="Set priority, match prompts, send to a pane, or run Lua."
        expanded={advanced}
        aria-controls={advancedId}
        onClick={() => setAdvanced((open) => !open)}
      />
      {advanced && (
        <div id={advancedId} className="st-auto-advanced">
          <TriggerAdvanced t={t} update={update} locked={locked} style={style} />
        </div>
      )}
    </Card>
  );
}

function TriggerAdvanced({
  t,
  update,
  locked,
  style,
}: {
  t: TriggerRecord;
  update: DetailProps<TriggerRecord>['update'];
  locked: boolean;
  style: TriggerStyle;
}) {
  const setActions = (fn: (actions: TriggerAction[]) => TriggerAction[]) =>
    update((v) => ({ ...v, actions: fn(v.actions) }));
  const setHighlight = <K extends keyof HighlightStyle>(key: K, value: HighlightStyle[K]) =>
    setActions((a) => withHighlight(a, { [key]: value } as HighlightPatch));
  const highlight = highlightOf(t.actions);
  const extras = extraEffects(t.actions);

  return (
    <>
      <Row label="Priority" description="Vosh runs triggers with higher numbers first.">
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
        description="Prompts match what your MUD sends before you type. Room matches the armies, things and people a room lists after its exits. Your target matches the line of the one you target with tar when a room lists them."
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
      <PatternsBlock t={t} update={update} locked={locked} />
      {highlight && (style === 'highlight' || style === 'wash') && (
        <>
          <Row label="Text color">
            <Select
              width={160}
              value={highlight.fg ?? ''}
              options={colorOptions(highlight.fg)}
              disabled={locked}
              onChange={(fg) => setHighlight('fg', (fg || undefined) as NamedColor | undefined)}
            />
          </Row>
          <Row label="Background">
            <Select
              width={160}
              value={highlight.bg ?? ''}
              options={colorOptions(highlight.bg)}
              disabled={locked}
              onChange={(bg) => setHighlight('bg', (bg || undefined) as NamedColor | undefined)}
            />
          </Row>
          <Row label="Bold">
            <Toggle
              checked={Boolean(highlight.bold)}
              disabled={locked}
              onChange={(on) => setHighlight('bold', on)}
            />
          </Row>
          <Row label="Underline">
            <Toggle
              checked={Boolean(highlight.underline)}
              disabled={locked}
              onChange={(on) => setHighlight('underline', on)}
            />
          </Row>
          <Row label="Inverse">
            <Toggle
              checked={Boolean(highlight.inverse)}
              disabled={locked}
              onChange={(on) => setHighlight('inverse', on)}
            />
          </Row>
        </>
      )}
      <Row label="Send to pane" description="The pane that also gets the matching line, like chat.">
        <Field
          width="100%"
          value={effectOf(t.actions, 'route')}
          placeholder="No pane"
          disabled={locked}
          onChange={(pane) => setActions((a) => withEffect(a, 'route', pane))}
        />
      </Row>
      <CodeRow
        label="Lua script"
        description="Runs on each match. The captures table holds what the pattern caught."
        value={effectOf(t.actions, 'script')}
        readOnly={locked}
        onChange={(body) => setActions((a) => withEffect(a, 'script', body))}
      />
      {extras.map((extra) => (
        <Row key={extra.index} label={EFFECT_LABELS[extra.kind]}>
          <FieldArea
            className="st-auto-grow"
            mono={extra.kind !== 'route'}
            width="100%"
            value={extra.value}
            disabled={locked}
            onChange={(next) => setActions((a) => withEffectAt(a, extra.index, next))}
          />
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
      ))}
    </>
  );
}

/** The patterns past the main one. Each has its own switch, and the
 *  trigger fires when any pattern that is on matches. */
function PatternsBlock({
  t,
  update,
  locked,
}: {
  t: TriggerRecord;
  update: DetailProps<TriggerRecord>['update'];
  locked: boolean;
}) {
  const headingId = useId();
  const main = mainPattern(t);
  const extra = t.patterns.slice(1);
  const setPatterns = (fn: (rest: TriggerPattern[]) => TriggerPattern[]) =>
    update((v) => {
      const first = v.patterns[0] ?? mainPattern(v);
      return { ...v, patterns: [first, ...fn(v.patterns.slice(1))] };
    });

  return (
    <div className="st-row st-auto-block" role="group" aria-labelledby={headingId}>
      <div className="st-row-text">
        <span id={headingId} className="st-row-label">
          More patterns
        </span>
        <span className="st-row-desc">The trigger fires when any pattern that is on matches.</span>
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
            onClick={() => setPatterns((rest) => [...rest, { pattern: '', enabled: true }])}
          >
            Add pattern
          </ChipButton>
        </div>
      )}
    </div>
  );
}
