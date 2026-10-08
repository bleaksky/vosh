import { Fragment, useId, type ReactNode } from 'react';
import { countPhrase } from '../../automation/automationDraft';
import {
  blankPattern,
  mainPattern,
  MATCH_MODE_DESCRIPTIONS,
  MATCH_MODE_OPTIONS,
  patternSource,
  triggerMode,
  withMainPattern,
  withMainPatternSource,
  withPatternSource,
  withTriggerMode,
} from '../../automation/automationTriggers';
import { patternKey, type TriggerCard } from '../../automation/presetEdits';
import type { TriggerPattern, TriggerRecord } from '../../ipc/automation';
import type { EditValue } from '../../ipc/presetEdits';
import { ChipButton, CloseIcon, Field, PlusIcon, Segmented, Toggle } from '../../ui';
import {
  mainKeyOf,
  morePatternsDiffer,
  PATTERN_NOUN,
  presetSwitch,
  presetText,
  withChanged,
  type Flag,
} from './triggerChanged';
import { Changed } from './TriggerChangeNotes';
import type { DetailProps } from './types';

/** Whether a pattern row's value has the pattern on. */
const patternOn = (value: EditValue) =>
  typeof value === 'object' && !Array.isArray(value) && value.enabled !== false;

const modeLabel = (value: EditValue) =>
  MATCH_MODE_OPTIONS.find((o) => o.value === value)?.label ?? String(value);

/** The Pattern row: the label and what the mode does on the left with
 *  the mode beside them, and the main pattern at full width under both.
 *  Built by hand like CodeRow, since a Row keeps its control on the
 *  right of the label. */
export function PatternRow({
  t,
  update,
  locked,
  ship,
  flag,
}: {
  t: TriggerCard;
  update: DetailProps<TriggerCard>['update'];
  locked: boolean;
  ship: TriggerRecord | null;
  flag: Flag;
}) {
  const labelId = useId();
  const descId = useId();
  const mode = triggerMode(t);
  const source = patternSource(mainPattern(t));
  // A fix to the main pattern or its mode shows its choices in place of
  // the line that says what the preset has.
  let changed: ReactNode =
    (ship && flag(mainKeyOf(ship), (v) => presetSwitch(patternOn(v)))) ?? flag('mode', modeLabel);
  if (changed === null && ship && patternSource(mainPattern(ship)) !== source) {
    changed = <Changed>{presetText(patternSource(mainPattern(ship)))}</Changed>;
  } else if (changed === null && ship && triggerMode(ship) !== mode) {
    changed = <Changed>{modeLabel(triggerMode(ship))}</Changed>;
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

/** The patterns past the main one. Each has its own switch, and the
 *  trigger fires when any pattern that is on matches. */
export function PatternsBlock({
  t,
  update,
  locked,
  ship,
  flag,
}: {
  t: TriggerCard;
  update: DetailProps<TriggerCard>['update'];
  locked: boolean;
  ship: TriggerRecord | null;
  flag: Flag;
}) {
  const headingId = useId();
  const main = mainPattern(t);
  const extra = t.patterns.slice(1);
  // A fix that turned one of the preset's patterns past the main one on
  // or off under your edit.
  const fixes = (ship?.patterns.slice(1) ?? []).map((p) => {
    const text = patternSource(p);
    return flag(patternKey(text), (v) => (
      <>
        <span className="st-auto-mono">{text}</span> {patternOn(v) ? 'on' : 'off'}
      </>
    ));
  });
  const changed = fixes.some((f) => f !== null) ? (
    fixes.map((f, i) => <Fragment key={i}>{f}</Fragment>)
  ) : ship && morePatternsDiffer(t, ship) ? (
    <Changed>{countPhrase(ship.patterns.length, PATTERN_NOUN)}</Changed>
  ) : null;
  const setPatterns = (fn: (rest: TriggerPattern[], v: TriggerCard) => TriggerPattern[]) =>
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
          {withChanged('The trigger fires when any pattern that is on matches.', changed)}
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
