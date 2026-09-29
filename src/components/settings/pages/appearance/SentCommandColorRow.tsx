import type { UiConfig } from '../../../../lib/session';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';
import { Button, ColorField, Row } from '../../ui';

// One row with no ties to the page around it, so it can move to
// another group (Input is the likely home) as it is.

interface SentCommandColorRowProps {
  config: UiConfig;
  update: UpdateConfig;
}

/** The color of the commands you send as the terminal echoes them.
 *  Empty keeps the terminal's text color. */
export function SentCommandColorRow({ config, update }: SentCommandColorRowProps) {
  const value = config.input_echo_color ?? '';
  return (
    <Row
      anchor="sent-color"
      label="Sent command color"
      description="The color of each command you send when the terminal shows it."
    >
      <ColorField
        value={value}
        allowEmpty
        placeholder="Theme color"
        emptySwatch="var(--text)"
        pickerLabel="Choose the sent command color"
        onChange={(color) => update({ input_echo_color: color || null })}
      />
      <Button
        disabled={value === ''}
        onClick={() => update({ input_echo_color: null }, { now: true })}
      >
        Reset
      </Button>
    </Row>
  );
}
