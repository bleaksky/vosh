import { normalizeStatusStyle, STATUS_STYLES, type StatusStyle } from '../../ipc/uiConfig';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Select, type SelectOption } from '../../ui';
import { STATUS_STYLE_LABELS } from './statusStyleLabels';

// How the main window's status bar draws, from UiConfig status_style.
// It writes through the same debounced save as the other rows, which
// tells every window, so the bar follows at once. Its search anchor is
// layout:status#status-style. Four names do not fit beside the row's
// words as segments in the narrowest Settings window, so the styles sit
// in a select.

const DESCRIPTIONS: Readonly<Record<StatusStyle, string>> = {
  meters:
    'Your vitals and the tick fill the bar from edge to edge, so you can read them at a glance.',
  compact: 'A quiet line of values in small text. Tick and time below picks its labels.',
  strip: 'Each vital and the tick get a small gauge on a raised bar.',
  dashboard: 'A taller bar with a caption over each value and a bar under each vital.',
};

const OPTIONS: readonly SelectOption[] = STATUS_STYLES.map((value) => ({
  value,
  label: STATUS_STYLE_LABELS[value],
}));

type StatusStyleRowProps = Pick<SettingsPageProps, 'config' | 'setConfig' | 'onError'>;

export function StatusStyleRow({ config, setConfig, onError }: StatusStyleRowProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const style = config?.status_style ?? 'meters';
  return (
    <Row label="Style" description={DESCRIPTIONS[style]} anchor="status-style">
      <Select
        options={OPTIONS}
        value={style}
        disabled={!config}
        onChange={(value) => update({ status_style: normalizeStatusStyle(value) })}
      />
    </Row>
  );
}
