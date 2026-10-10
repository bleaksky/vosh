import type { ChipStyle } from '../../ipc/uiConfig';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { STATUS_STYLE_LABELS } from './statusStyleLabels';
import { Row, Segmented, type SegmentedOption } from '../../ui';

// How the tick, the game time, and the moons show in the Compact
// status bar, from UiConfig chip_style. The other styles label every
// reading their own way, so the row rests while one of them is on. It
// writes through the same
// debounced save as the other rows, which tells every window, so the
// status line follows at once. Its search anchor is layout:status#tick-time,
// so it belongs in a Layout section with the id `status`.

const OPTIONS: readonly SegmentedOption<ChipStyle>[] = [
  { value: 'value_only', label: 'Value' },
  { value: 'caption_value', label: 'Caption' },
  { value: 'icon_value', label: 'Icon' },
];

type TickTimeStyleRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function TickTimeStyleRow({ config, setConfig, onError }: TickTimeStyleRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  // Nothing to press until the profile's config loads, or while
  // another style labels the readings.
  const compact = config?.status_style === 'compact';
  const options = compact ? OPTIONS : OPTIONS.map((option) => ({ ...option, disabled: true }));
  return (
    <Row
      label="Tick and time"
      description={
        !config || compact
          ? 'How the tick, the game time, and the moons show when the status line is Compact.'
          : `${STATUS_STYLE_LABELS[config.status_style]} labels the tick and the time on its own. Pick Compact to choose.`
      }
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
