import type { ChipStyle } from '../../../ipc/uiConfig';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Segmented, type SegmentedOption } from '../ui';

// How the tick, the game time, and the moons show in the main window's
// status line, from UiConfig chip_style. It writes through the same
// debounced save as the other rows, and setUiConfig tells every window,
// so the status line follows at once. Its search anchor is layout:status#tick-time,
// so it belongs in a Layout section with the id `status`.

const OPTIONS: readonly SegmentedOption<ChipStyle>[] = [
  { value: 'value_only', label: 'Value' },
  { value: 'caption_value', label: 'Caption' },
  { value: 'icon_value', label: 'Icon' },
];

type TickTimeStyleRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function TickTimeStyleRow({ config, setConfig, onError }: TickTimeStyleRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  // Nothing to press until the profile's config loads.
  const options = config ? OPTIONS : OPTIONS.map((option) => ({ ...option, disabled: true }));
  return (
    <Row
      label="Tick and time"
      description="How the tick, the game time, and the moons show in the status line."
      anchor="tick-time"
    >
      <Segmented
        options={options}
        value={config?.chip_style ?? null}
        onChange={(chip_style) => update({ chip_style })}
      />
    </Row>
  );
}
