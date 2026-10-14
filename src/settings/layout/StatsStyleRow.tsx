import { STATS_STYLES, type StatsStyle } from '../../ipc/uiConfig';
import { STATS_STYLE_LABELS } from '../../panel/stats/statsView';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Segmented, type SegmentedOption } from '../../ui';

// How the Stats pane draws, from UiConfig stats_style. It writes through
// the same debounced save as the other rows, which tells every window,
// so the pane follows at once. The pane's own menu picks it too. Its
// search anchor is layout:stats#stats-style.

const DESCRIPTIONS: Readonly<Record<StatsStyle, string>> = {
  sheet:
    'Two columns laid out like score, with each stat the way the game writes it, trained and then current in brackets.',
  rows: 'A row for each number, with a small meter beside your stats, your armor and your hitroll.',
  ledger: 'Big figures under small captions, like the Ledger vitals, with a line under each stat.',
};

const OPTIONS: readonly SegmentedOption<StatsStyle>[] = STATS_STYLES.map((value) => ({
  value,
  label: STATS_STYLE_LABELS[value],
}));

type StatsStyleRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function StatsStyleRow({ config, setConfig, onError }: StatsStyleRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const style = config?.stats_style ?? 'sheet';
  const options = config ? OPTIONS : OPTIONS.map((o) => ({ ...o, disabled: true }));
  return (
    <Row label="Style" description={DESCRIPTIONS[style]} anchor="stats-style">
      <Segmented
        options={options}
        value={style}
        onChange={(value) => update({ stats_style: value })}
      />
    </Row>
  );
}
