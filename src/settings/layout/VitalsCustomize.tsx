import { useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import {
  DEFAULT_VITALS_CUSTOM,
  normalizeVitalsOff,
  VITALS_VALUES,
  shownStyle,
  type UiConfig,
  type UiFields,
  type Vital,
  type VitalsMeter,
  type VitalsOpponent,
  type VitalsStyle,
  type VitalsColors,
  type VitalsValues,
} from '../../ipc/uiConfig';
import { partShift, useRowDrag } from '../../lib/useRowDrag';
import { VITAL_LABELS, VITALS_VALUES_LABELS } from '../../panel/vitalsView';
import type { AnsiSlot } from '../../theme/baseAnsi';
import { playPalette, themeTokens, type XtermPalette } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';
import {
  Button,
  cx,
  GripIcon,
  Row,
  Section,
  Segmented,
  Toggle,
  VisuallyHidden,
  type SegmentedOption,
} from '../../ui';
import { colorMarks, type ColorMark } from './vitalColorMarks';
import { VitalSwatch } from './VitalSwatch';
import { VitalsTextRows } from './VitalsTextRows';
import { customDiffers, movedTo, movedWords, textDiffers } from './vitalsStyles';

// Customize vitals under Settings, Layout. One set of choices every
// drawn style shares: which vitals show and their order, a color for
// each, where your opponent sits, Values, Meter and the warning. Each
// pick saves alone, and Reset to default puts the set back and leaves
// your style, Show your vitals in and the pinned switch alone. Show each
// hit sits with the warning, since both change how a vital reads as it
// changes. It rests until something differs.
//
// The list moves a vital with its grip, by the pointer or from the
// keyboard, as the Sessions list moves a session. Under Status line the
// swatches and Meter go quiet, since the line keeps quiet labels and
// draws no meter, and they keep your picks for the panel. Gauges and
// Pips draw their own mark, so Meter goes quiet for them too. Under Text
// your text decides all of it, so the section holds your text and its
// preview, and Reset to default puts back the text Text starts from.

/** The rows' pitch, a 40 px row with no gap. */
const ROW_PITCH = 40;

const OPPONENT_PLACES: readonly SegmentedOption<VitalsOpponent>[] = [
  { value: 'top', label: 'On top' },
  { value: 'bottom', label: 'At the bottom' },
];

const VALUES: readonly SegmentedOption<VitalsValues>[] = VITALS_VALUES.map((value) => ({
  value,
  label: VITALS_VALUES_LABELS[value],
}));

const METERS: readonly SegmentedOption<VitalsMeter>[] = [
  { value: 'line', label: 'Line' },
  { value: 'bar', label: 'Bar' },
  { value: 'none', label: 'None' },
];

/** Why Meter goes quiet for each style that draws its own mark. */
const OWN_MARKS: Partial<Record<VitalsStyle, string>> = {
  gauges: 'Gauges draw their own pill, so they take no meter.',
  pips: 'Pips draw their own discs, so they take no meter.',
  bands: 'Bands draw their own bars, so they take no meter.',
  ladders: 'Ladders draw their own segments, so they take no meter.',
  blocks: 'Blocks draw their own cells, so they take no meter.',
  traces: 'Traces draw their own line, so they take no meter.',
  dials: 'Dials draw their own arc, so they take no meter.',
  rings: 'Rings draw their own arcs, so they take no meter.',
  vials: 'Vials draw their own glass, so they take no meter.',
  orbs: 'Orbs draw their own glass, so they take no meter.',
  candles: 'Candles draw their own wax, so they take no meter.',
};

/** What Show each hit does. */
const HIT_WORDS =
  'A hit leaves the part it took pale for a moment, then it drains away. Works in every style with a fill.';

/** Why Show each hit goes quiet, or null where it draws. */
function hitQuietOf(style: VitalsStyle, status: boolean): string | null {
  if (status) return "The status line doesn't show hits, so this waits for the panel.";
  if (style === 'traces') return 'Traces already draw each hit in their line.';
  return null;
}

/** Why Meter goes quiet for a style or the status line, or null where it
 *  draws. */
function meterQuiet(style: VitalsStyle, status: boolean): string | null {
  if (status) return 'The status line draws no meter.';
  return OWN_MARKS[style] ?? null;
}

export function CustomizeVitalsSection({
  config,
  update,
}: {
  config: UiConfig;
  update: (patch: UiFields) => void;
}) {
  const text = shownStyle(config) === 'text';
  const differs = text
    ? textDiffers(config.vitals_text, config.vitals_legacy_text)
    : customDiffers(config);
  const reset = () =>
    update(
      text
        ? { vitals_text: '' }
        : { ...DEFAULT_VITALS_CUSTOM, vitals_order: [...DEFAULT_VITALS_CUSTOM.vitals_order] },
    );
  return (
    <Section
      id="customize-vitals"
      title="Customize vitals"
      actions={
        <Button disabled={!differs} onClick={reset}>
          Reset to default
        </Button>
      }
    >
      {text ? <VitalsTextRows config={config} /> : <CustomRows config={config} update={update} />}
    </Section>
  );
}

/** The rows a drawn style shares: the list, your opponent, Values, Meter
 *  and the warning. */
function CustomRows({ config, update }: { config: UiConfig; update: (patch: UiFields) => void }) {
  const status = config.vitals_place === 'status';
  const quiet = meterQuiet(shownStyle(config), status);
  const hitQuiet = hitQuietOf(shownStyle(config), status);
  const opponentOn = !config.vitals_off.includes('opponent');
  return (
    <>
      <div className="st-block st-vitals-head" data-st-anchor="vitals-order">
        <div className="st-row-text">
          <span className="st-row-label">Vitals and their order</span>
          <span className="st-row-desc">
            {status
              ? 'Drag a vital to move it. Turn one off to drop it from the status line. The status line keeps its quiet labels, so your colors wait for the panel.'
              : 'Drag a vital to move it. Turn one off to drop it from the panel.'}
          </span>
        </div>
      </div>
      <VitalsList config={config} quietColors={status} update={update} />
      <Row
        label="Your opponent"
        description="In a fight, its name and its health in warn, in every style."
        anchor="opponent"
      >
        <Segmented
          options={OPPONENT_PLACES}
          value={config.vitals_opponent}
          onChange={(place) => update({ vitals_opponent: place })}
        />
        <Toggle
          checked={opponentOn}
          aria-label="Show your opponent"
          onChange={(on) => update({ vitals_off: switched(config.vitals_off, 'opponent', on) })}
        />
      </Row>
      <Row
        label="Values"
        description="Current drops the maximum. Percent matches the Group pane."
        anchor="values"
      >
        <Segmented
          options={VALUES}
          value={config.vitals_values}
          onChange={(values) => update({ vitals_values: values })}
        />
      </Row>
      <Row
        label="Meter"
        description={quiet ?? 'Bar is easier to read in a fight. None keeps only the numbers.'}
        anchor="meter"
      >
        <Segmented
          options={quiet ? METERS.map((option) => ({ ...option, disabled: true })) : METERS}
          value={config.vitals_meter}
          onChange={(meter) => update({ vitals_meter: meter })}
        />
      </Row>
      <Row
        label="Warn before you run low"
        description="Vitals turn yellow under two thirds and red under one third, like your group's health."
        anchor="warn-low"
      >
        <Toggle
          checked={config.vitals_warn_thirds}
          onChange={(on) => update({ vitals_warn_thirds: on })}
        />
      </Row>
      <Row label="Show each hit" description={hitQuiet ?? HIT_WORDS} anchor="show-each-hit">
        <Toggle
          checked={config.vitals_hit}
          disabled={hitQuiet !== null}
          onChange={(on) => update({ vitals_hit: on })}
        />
      </Row>
    </>
  );
}

/** `off` with `name` turned on or off. */
function switched<T extends string>(off: readonly T[], name: T, on: boolean) {
  return normalizeVitalsOff(on ? off.filter((n) => n !== name) : [...off, name]);
}

/** `colors` with `vital` on `slot`, or on Default for null. */
function colored(colors: VitalsColors, vital: Vital, slot: number | null): VitalsColors {
  const { [vital]: _was, ...rest } = colors;
  return slot === null ? rest : { ...rest, [vital]: slot };
}

/** Vitals and their order: a row for each vital with its grip, its name,
 *  its color and its switch. Exported for its test. */
export function VitalsList({
  config,
  quietColors,
  update,
}: {
  config: UiConfig;
  /** The swatches rest while the status line shows your vitals. */
  quietColors: boolean;
  update: (patch: UiFields) => void;
}) {
  const theme = useActiveTheme();
  const palette = playPalette(theme, config.fit_game_colors, config.color_vision);
  const marks = colorMarks(
    palette,
    themeTokens(theme, config.color_vision),
    config.color_vision,
    config.vitals_warn_thirds,
  );
  const list = useRef<HTMLUListElement | null>(null);
  const order = config.vitals_order;
  const [said, setSaid] = useState('');
  const { drag, press, keyDown, putBack } = useRowDrag(
    list,
    order,
    (vital, to) => {
      update({ vitals_order: movedTo(order, vital, to) });
      setSaid(movedWords(vital, to, order.length));
    },
    ROW_PITCH,
  );
  return (
    <>
      <ul
        ref={list}
        className={drag ? 'st-vitals-list is-dragging' : 'st-vitals-list'}
        aria-label="Vitals and their order"
      >
        {order.map((vital, i) => (
          <VitalRow
            key={vital}
            vital={vital}
            on={!config.vitals_off.includes(vital)}
            slot={config.vitals_colors[vital]}
            palette={palette}
            marks={marks}
            quietColor={quietColors}
            lifted={drag?.id === vital}
            offset={
              drag ? (drag.id === vital ? drag.dy : partShift(i, drag.from, drag.to, ROW_PITCH)) : 0
            }
            onPress={(e) => press(e, vital)}
            onKeyDown={(e) => keyDown(e, vital)}
            onBlur={putBack}
            onSwitch={(on) => update({ vitals_off: switched(config.vitals_off, vital, on) })}
            onColor={(slot) =>
              update({ vitals_colors: colored(config.vitals_colors, vital, slot) })
            }
          />
        ))}
        {drag && drag.to !== drag.from && (
          <li
            className="st-vitals-drop"
            aria-hidden="true"
            // The line sits between the place the row lands on and the
            // rows that part for it.
            style={{ top: (drag.to < drag.from ? drag.to + 1 : drag.to) * ROW_PITCH }}
          />
        )}
      </ul>
      <span aria-live="polite">
        <VisuallyHidden>{said}</VisuallyHidden>
      </span>
    </>
  );
}

function VitalRow({
  vital,
  on,
  slot,
  palette,
  marks,
  quietColor,
  lifted,
  offset,
  onPress,
  onKeyDown,
  onBlur,
  onSwitch,
  onColor,
}: {
  vital: Vital;
  on: boolean;
  /** The ANSI slot you picked, or undefined for Default. */
  slot: number | undefined;
  palette: XtermPalette;
  marks: Partial<Record<AnsiSlot, ColorMark>>;
  quietColor: boolean;
  lifted: boolean;
  offset: number;
  onPress: (e: PointerEvent) => void;
  onKeyDown: (e: KeyboardEvent) => void;
  onBlur: () => void;
  onSwitch: (on: boolean) => void;
  onColor: (slot: number | null) => void;
}) {
  const label = VITAL_LABELS[vital];
  return (
    <li
      className={cx('st-vital', !on && 'is-off', lifted && 'is-lifted')}
      style={offset ? { transform: `translateY(${offset}px)` } : undefined}
    >
      <span
        className="st-vital-grip"
        role="button"
        tabIndex={0}
        aria-label={`Move ${label}`}
        onPointerDown={onPress}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
      >
        <GripIcon />
      </span>
      <span className="st-vital-name">{label}</span>
      <VitalSwatch
        vital={vital}
        slot={slot}
        palette={palette}
        marks={marks}
        disabled={quietColor || !on}
        onPick={onColor}
      />
      <Toggle checked={on} aria-label={`Show ${label}`} onChange={onSwitch} />
    </li>
  );
}
