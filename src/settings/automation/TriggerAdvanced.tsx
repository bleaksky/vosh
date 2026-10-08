import type { ReactNode } from 'react';
import { withAlertParts } from '../../automation/alertParts';
import {
  effectOf,
  extraEffects,
  HIGHLIGHT_COLORS,
  highlightOf,
  withEffect,
  withEffectAt,
  withHighlight,
  type HighlightPatch,
  type TriggerStyle,
} from '../../automation/automationTriggers';
import type { TriggerCard } from '../../automation/presetEdits';
import type { HighlightStyle, NamedColor, TriggerAction } from '../../ipc/automation';
import type { EditValue } from '../../ipc/presetEdits';
import {
  CloseIcon,
  Field,
  FieldArea,
  Row,
  Segmented,
  Select,
  Toggle,
  type SelectOption,
} from '../../ui';
import { AlertDetailRows } from './AlertRows';
import { CodeRow, NumberField } from './fields';
import {
  alsoSends,
  presetSwitch,
  presetText,
  withAlert,
  withChanged,
  type Flag,
  type FromPreset,
} from './triggerChanged';
import { Changed, UnderField } from './TriggerChangeNotes';
import { PatternsBlock } from './TriggerPatterns';
import type { DetailProps } from './types';

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

export function TriggerAdvanced({
  t,
  update,
  locked,
  style,
  from,
  changed,
  flag,
}: {
  t: TriggerCard;
  update: DetailProps<TriggerCard>['update'];
  locked: boolean;
  style: TriggerStyle;
  from: FromPreset | null;
  changed: (key: string, say: (value: EditValue) => ReactNode) => ReactNode;
  flag: Flag;
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
      <PatternsBlock t={t} update={update} locked={locked} ship={from?.ship ?? null} flag={flag} />
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
