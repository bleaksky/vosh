import type { GameTime, UiConfig } from '../../../ipc/uiConfig';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Segmented, type SegmentedOption } from '../ui';

// The clock the main window's status line reads the game time on, from
// UiConfig game_time. It sits under the Tick and time row and writes
// through the same debounced save, and setUiConfig tells every window,
// so the status line follows at once. Its search anchor is
// layout:status#game-time, so it belongs in the Layout section with the
// id `status`.

const OPTIONS: readonly SegmentedOption<GameTime>[] = [
  { value: '24h', label: '24 hour' },
  { value: '12h', label: '12 hour' },
];

type GameTimeRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function GameTimeRow({ config, setConfig, onError }: GameTimeRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  return <GameTimeField config={config} update={update} />;
}

interface GameTimeFieldProps {
  /** The profile's config, or null until it loads. */
  config: UiConfig | null;
  update: (patch: Partial<UiConfig>) => void;
}

/** The row itself, drawn from the config it is handed. Exported for its
 *  test. */
export function GameTimeField({ config, update }: GameTimeFieldProps) {
  // Nothing to press until the profile's config loads.
  const options = config ? OPTIONS : OPTIONS.map((option) => ({ ...option, disabled: true }));
  return (
    <Row
      label="Game time"
      description="How the game time shows in the status line, like 18:00 or 6:00 PM."
      anchor="game-time"
    >
      <Segmented
        options={options}
        value={config?.game_time ?? null}
        onChange={(game_time) => update({ game_time })}
      />
    </Row>
  );
}
