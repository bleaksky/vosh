import { useEffect, useState } from 'react';
import { AFFECTS_MARKER_LABELS, AFFECTS_STYLE_LABELS } from '../../../lib/affectsDisplay';
import APP_SHORTCUTS from '../../../lib/appShortcuts.json';
import { PANEL_WIDTH_MAX, PANEL_WIDTH_MIN } from '../../../lib/paneLayout';
import { isMacPlatform, shortcutKeys, shortcutLabel } from '../../../lib/palette';
import { profilePossessive } from '../../../lib/profileLabel';
import {
  AFFECTS_HOURS_MAX,
  AFFECTS_MARKERS,
  AFFECTS_STYLES,
  isChipsStyle,
  normalizeAffectsStyle,
  profilesList,
  subscribeProfileSwitched,
  subscribeProfilesChanged,
  type AffectsMarker,
  type UiConfig,
  type VitalsDensity,
  type VitalsMeter,
  type VitalsValues,
} from '../../../lib/session';
import {
  panelWidthOf,
  setPanelOpen,
  setPanelWidth,
  usePanelLayout,
} from '../../panel/panelLayoutStore';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { TickCountRow } from '../rows/TickCountRow';
import { TickTimeStyleRow } from '../rows/TickTimeStyleRow';
import {
  ColorField,
  Keycap,
  LinkRow,
  NumberField,
  Row,
  Section,
  Segmented,
  Select,
  Toggle,
  type SegmentedOption,
  type SelectOption,
} from '../ui';

// Settings, Layout (SettingsLayout.dc.html). How the window is
// arranged. The Panel card edits the panel of the character you are
// playing, named in its heading, and follows a profile switch live.
// Show the panel and Width write that profile's pane layout through
// the panel layout store, so the main window follows at once. What
// each character keeps, its panes and tracked affects, stays in
// Characters, which the last Panel row opens. Affects, Vitals, Split
// terminal, and Status line save with the rest of the config. Each
// profile keeps its own, but they say how a pane draws, not what a
// character tracks, so they sit here beside the Vitals and Tick counts
// rows that work the same way.

// The keycaps read the shortcut table the menu bar and the palette
// read, so every place shows the same keys.
const PANEL_KEYS = APP_SHORTCUTS.panel;
const SPLIT_KEYS = APP_SHORTCUTS.split;

/** The live profile's name, or null until it loads. Follows a switch
 *  and a rename. */
function useActiveProfile(): string | null {
  const [active, setActive] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    const unsubs: (() => void)[] = [];
    const reload = () =>
      profilesList()
        .then((list) => {
          if (!cancelled) setActive(list.active);
        })
        .catch(() => {});
    void reload();
    const keep = (pending: Promise<() => void>) =>
      void pending.then((fn) => {
        if (cancelled) fn();
        else unsubs.push(fn);
      });
    keep(
      subscribeProfileSwitched((name) => {
        if (!cancelled) setActive(name);
      }),
    );
    keep(
      subscribeProfilesChanged(() => {
        if (!cancelled) void reload();
      }),
    );
    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);
  return active;
}

export function LayoutGroup({ config, setConfig, onError, navigate }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const mac = isMacPlatform();
  const profile = useActiveProfile();
  const layout = usePanelLayout();
  const owner = profile === null ? null : profilePossessive(profile);

  return (
    <>
      <Section id="panel" title={owner === null ? 'Panel' : `${owner} panel`}>
        <Row
          label="Show the panel"
          description="When you hide it, your vitals move to the status line."
          anchor="show-panel"
        >
          <span className="st-control-group">
            <span className="st-keys" aria-hidden="true">
              {shortcutKeys(PANEL_KEYS, mac).map((key) => (
                <Keycap key={key}>{key}</Keycap>
              ))}
            </span>
            <Toggle
              checked={layout?.panel_open ?? true}
              disabled={layout === null}
              aria-keyshortcuts={mac ? 'Meta+Shift+L' : 'Control+Shift+L'}
              onChange={setPanelOpen}
            />
          </span>
        </Row>
        <Row label="Width" description="You can also drag the panel's edge." anchor="panel-width">
          <NumberField
            value={panelWidthOf(layout)}
            disabled={layout === null}
            onChange={setPanelWidth}
            min={PANEL_WIDTH_MIN}
            max={PANEL_WIDTH_MAX}
            step={10}
            unit="pt"
            unitName="points"
          />
        </Row>
        <LinkRow
          label="Panes and tracked affects"
          description={
            owner === null
              ? 'Vosh saves these for each character. Open Characters to change them.'
              : `Vosh saves these for each character. Open Characters to change ${owner}.`
          }
          anchor="panes"
          onClick={() =>
            navigate(
              profile === null
                ? { group: 'characters' }
                : { group: 'characters', section: profile },
            )
          }
        />
      </Section>

      {config && <AffectsSection config={config} update={update} />}

      {config && <VitalsSection config={config} update={update} />}

      {config && (
        <Section id="split" title="Split terminal">
          <Row
            label="Divider color"
            description={`Scroll up or press ${shortcutLabel(SPLIT_KEYS, mac)} to split the terminal.`}
            anchor="divider-color"
          >
            <ColorField
              value={config.split_divider_color ?? ''}
              onChange={(color) => update({ split_divider_color: color || null })}
              allowEmpty
              // The native divider reads only hex and rgb (parse_css_color
              // in src-tauri/src/color.rs), so the field saves #rrggbb.
              hexOnly
              placeholder="Theme default"
              // The divider the terminal draws while no color is set:
              // the native grid's hairline on macOS, the tertiary tone
              // in the web terminal elsewhere.
              emptySwatch={mac ? 'var(--sep)' : 'var(--tertiary)'}
              pickerLabel="Choose a divider color"
            />
          </Row>
        </Section>
      )}

      <StatusLineSection config={config} setConfig={setConfig} onError={onError} />
    </>
  );
}

/** The Status line card: how the tick, the time, and the moons show,
 *  and under it which way the tick counts. Exported for its test. */
export function StatusLineSection({
  config,
  setConfig,
  onError,
}: Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>) {
  return (
    <Section
      id="status"
      title="Status line"
      help={{ topic: 'tick.tick-timer', subject: 'the tick timer' }}
    >
      <TickTimeStyleRow config={config} setConfig={setConfig} onError={onError} />
      <TickCountRow config={config} setConfig={setConfig} onError={onError} />
    </Section>
  );
}

// Four names do not fit beside the row's words as segments in the
// narrowest Settings window, so the styles sit in a select.
const AFFECTS_STYLE_OPTIONS: readonly SelectOption[] = AFFECTS_STYLES.map((value) => ({
  value,
  label: AFFECTS_STYLE_LABELS[value],
}));

/** A Marker segment's picture: the mark of an affect you have, then
 *  the mark of one you are missing, the pane's own 8 px shapes 4 px
 *  apart, in the segment's text color. */
function MarkerPicture({ marker }: { marker: AffectsMarker }) {
  return (
    <span className="st-marker" data-affects-marker={marker} aria-hidden="true">
      <span className="pane-affect-mark is-up" />
      <span className="pane-affect-mark is-missing" />
    </span>
  );
}

const MARKER_OPTIONS: readonly SegmentedOption<AffectsMarker>[] = AFFECTS_MARKERS.map((id) =>
  id === 'none'
    ? { value: id, label: AFFECTS_MARKER_LABELS[id] }
    : { value: id, name: AFFECTS_MARKER_LABELS[id], label: <MarkerPicture marker={id} /> },
);

/** How the Affects pane draws (AffectsStyles SPEC 4.1): one of the
 *  three approved boards or Draining chips, the mark beside each
 *  tracked affect, the wash behind what to recast, and the hours at
 *  which an affect runs out and is almost gone. Both chip styles show
 *  the state on each chip and always mark what to recast, so Marker
 *  and Tint go quiet while one is chosen and keep your picks for the
 *  other two. The hours apply to every style. Almost gone never goes
 *  over running out. Its field stops at running out, and running out
 *  set below it takes it down too, the way the backend coerces a hand
 *  edit. Exported for its test. */
export function AffectsSection({
  config,
  update,
}: {
  config: UiConfig;
  update: (patch: Partial<UiConfig>) => void;
}) {
  const chips = isChipsStyle(config.affects_style);
  const chipsName = AFFECTS_STYLE_LABELS[config.affects_style];
  const markers = chips
    ? MARKER_OPTIONS.map((option) => ({ ...option, disabled: true }))
    : MARKER_OPTIONS;
  return (
    <Section
      id="affects"
      title="Affects"
      help={{ topic: 'shape.group-affects', subject: 'the affects pane' }}
    >
      <Row
        label="Style"
        description="Timers first keeps your slots, Countdown sorts by hours left, Grouped chips puts what to recast first, and Draining chips colors only the hours a chip has left."
        anchor="affects-style"
      >
        <Select
          options={AFFECTS_STYLE_OPTIONS}
          value={config.affects_style}
          onChange={(style) => update({ affects_style: normalizeAffectsStyle(style) })}
        />
      </Row>
      <Row
        label="Marker"
        description={
          chips
            ? `${chipsName} show the state on each chip, so they draw no marker.`
            : 'It sits beside each affect you track, and its color shows whether the affect is up, running out, or missing.'
        }
        anchor="affects-marker"
      >
        <Segmented
          options={markers}
          value={config.affects_marker}
          onChange={(marker) => update({ affects_marker: marker })}
        />
      </Row>
      <Row
        label="Tint what to recast"
        description={
          chips
            ? `${chipsName} always mark what to recast.`
            : 'A missing affect sits on a red wash, and one about to drop sits on yellow or red.'
        }
        anchor="affects-tint"
      >
        <Toggle
          checked={config.affects_tint}
          disabled={chips}
          onChange={(on) => update({ affects_tint: on })}
        />
      </Row>
      <Row
        label="Running out at"
        description="With this many hours or fewer an affect's hours turn yellow, and one you track counts as running out."
        anchor="affects-running-out"
      >
        <NumberField
          value={config.affects_running_out_hours}
          onChange={(hours) =>
            update({
              affects_running_out_hours: hours,
              ...(hours < config.affects_almost_gone_hours
                ? { affects_almost_gone_hours: hours }
                : {}),
            })
          }
          min={0}
          max={AFFECTS_HOURS_MAX}
          unit="h"
          unitName="hours"
        />
      </Row>
      <Row
        label="Almost gone at"
        description="With this many hours or fewer the hours turn bold red. The game's own affects bar turns red at 1."
        anchor="affects-almost-gone"
      >
        <NumberField
          value={config.affects_almost_gone_hours}
          onChange={(hours) => update({ affects_almost_gone_hours: hours })}
          min={0}
          max={config.affects_running_out_hours}
          unit="h"
          unitName="hours"
        />
      </Row>
    </Section>
  );
}

const DENSITIES: readonly SegmentedOption<VitalsDensity>[] = [
  { value: 'rows', label: 'Rows' },
  { value: 'line', label: 'One line' },
];

const VALUES: readonly SegmentedOption<VitalsValues>[] = [
  { value: 'current-max', label: 'Current and max' },
  { value: 'current', label: 'Current' },
  { value: 'percent', label: 'Percent' },
];

const METERS: readonly SegmentedOption<VitalsMeter>[] = [
  { value: 'line', label: 'Line' },
  { value: 'bar', label: 'Bar' },
  { value: 'none', label: 'None' },
];

/** The vitals under the panel's panes (VitalsOptions.dc.html). Each
 *  default is the panel you had before these rows, so nothing changes
 *  until you pick something, except the last switch. It starts on and
 *  drops the vitals while your prompt is pinned above the command line,
 *  which usually shows them, and you can turn it off. The main window
 *  redraws as you click, so the card carries no preview. Values and the
 *  warning also shape the status line while the panel is hidden.
 *  Exported for its test. */
export function VitalsSection({
  config,
  update,
}: {
  config: UiConfig;
  update: (patch: Partial<UiConfig>) => void;
}) {
  return (
    <Section id="vitals" title="Vitals">
      <Row
        label="Density"
        description="One line fits Health, Mana, and Moves on a single row."
        anchor="density"
      >
        <Segmented
          options={DENSITIES}
          value={config.vitals_density}
          onChange={(density) => update({ vitals_density: density })}
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
        description="Bar is easier to read in a fight. None keeps only the numbers."
        anchor="meter"
      >
        <Segmented
          options={METERS}
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
      <Row
        label="Hide vitals while your prompt is pinned"
        description="While your prompt is pinned, the panes take their room. Turn it off if your prompt leaves your vitals out."
        anchor="hide-pinned"
      >
        <Toggle
          checked={config.vitals_hide_when_pinned}
          onChange={(on) => update({ vitals_hide_when_pinned: on })}
        />
      </Row>
    </Section>
  );
}
