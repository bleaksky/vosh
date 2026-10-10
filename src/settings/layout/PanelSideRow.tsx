import { PANEL_SIDES, type PanelSide } from '../../ipc/uiConfig';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Segmented, type SegmentedOption } from '../../ui';

// The side the panel sits on, from UiConfig panel_side. It writes
// through the same debounced save as the other rows, which tells every
// window, so the frame turns at once. Its search anchor is
// layout:panel#panel-side.

const LABELS: Readonly<Record<PanelSide, string>> = { right: 'Right', left: 'Left' };

const DESCRIPTIONS: Readonly<Record<PanelSide, string>> = {
  right: 'The panel sits on the right and your sessions on the left.',
  left: 'The panel sits on the left and your sessions move to the right.',
};

const OPTIONS: readonly SegmentedOption<PanelSide>[] = PANEL_SIDES.map((value) => ({
  value,
  label: LABELS[value],
}));

type PanelSideRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function PanelSideRow({ config, setConfig, onError }: PanelSideRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const side = config?.panel_side ?? 'right';
  const options = config ? OPTIONS : OPTIONS.map((o) => ({ ...o, disabled: true }));
  return (
    <Row label="Side" description={DESCRIPTIONS[side]} anchor="panel-side">
      <Segmented
        options={options}
        value={side}
        onChange={(value) => update({ panel_side: value })}
      />
    </Row>
  );
}
