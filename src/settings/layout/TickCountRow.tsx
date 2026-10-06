import type { TickCount, UiConfig } from '../../ipc/uiConfig';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Segmented, type SegmentedOption } from '../../ui';

// Which way the tick counts in the main window's status line, from
// UiConfig tick_count. It sits under the Tick and time row and writes
// through the same debounced save, which tells every window, so the
// status line follows at once. Its search anchor is
// layout:status#tick-counts, so it belongs in the Layout section with
// the id `status`.

const OPTIONS: readonly SegmentedOption<TickCount>[] = [
  { value: 'up', label: 'Up' },
  { value: 'down', label: 'Down' },
  { value: 'down_past_zero', label: 'Down past 0' },
];

type TickCountRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function TickCountRow({ config, setConfig, onError }: TickCountRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  return <TickCountField config={config} update={update} />;
}

interface TickCountFieldProps {
  /** The profile's config, or null until it loads. */
  config: UiConfig | null;
  update: (patch: Partial<UiConfig>) => void;
}

/** The row itself, drawn from the config it is handed. Exported for its
 *  test. */
export function TickCountField({ config, update }: TickCountFieldProps) {
  // Nothing to press until the profile's config loads.
  const options = config ? OPTIONS : OPTIONS.map((option) => ({ ...option, disabled: true }));
  return (
    <Row
      label="Tick counts"
      description="Up shows the seconds since the last tick and Down the seconds left until the next. Down waits at 0 when the game is late, and Down past 0 keeps counting below zero until the tick lands."
      anchor="tick-counts"
    >
      <Segmented
        options={options}
        value={config?.tick_count ?? null}
        onChange={(tick_count) => update({ tick_count })}
      />
    </Row>
  );
}
