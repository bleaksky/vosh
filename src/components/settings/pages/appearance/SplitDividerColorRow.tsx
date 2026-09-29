import type { UiConfig } from '../../../../lib/session';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';
import { Button, ColorField, Row } from '../../ui';

// One row with no ties to the page around it, so it can move to
// another group (Layout is the likely home) as it is.

interface SplitDividerColorRowProps {
  config: UiConfig;
  update: UpdateConfig;
}

/** The color of the line between live output and scrollback while the
 *  terminal is split. Empty follows the theme's hairline. */
export function SplitDividerColorRow({ config, update }: SplitDividerColorRowProps) {
  const value = config.split_divider_color ?? '';
  return (
    <Row
      anchor="divider-color"
      label="Split divider color"
      description="The line between live output and scrollback while you split the terminal."
    >
      <ColorField
        value={value}
        allowEmpty
        hexOnly
        placeholder="Theme color"
        emptySwatch="var(--sep)"
        pickerLabel="Choose the split divider color"
        onChange={(color) => update({ split_divider_color: color || null })}
      />
      <Button
        disabled={value === ''}
        onClick={() => update({ split_divider_color: null }, { now: true })}
      >
        Reset
      </Button>
    </Row>
  );
}
