import { ANSI_SLOT_LABELS, basePalette, withBaseColor } from '../../theme/appearanceSettings';
import { ANSI_SLOTS } from '../../theme/baseAnsi';
import type { UiConfig } from '../../ipc/uiConfig';
import type { UpdateConfig } from '../useSettingsAutoSave';
import { Button, Row } from '../../ui';
import { ColorBlock, ColorGroup } from './ColorGrid';

interface BasePaletteRowsProps {
  config: UiConfig;
  update: UpdateConfig;
}

/** The 16 colors MUD text uses while the theme's colors are off. The
 *  first change saves all 16, and Reset goes back to the stock chart. */
export function BasePaletteRows({ config, update }: BasePaletteRowsProps) {
  const saved = config.terminal_base_ansi;
  const colors = basePalette(saved);
  const slots = ANSI_SLOTS.map((key, i) => ({
    key,
    label: ANSI_SLOT_LABELS[key],
    value: colors[i],
  }));
  return (
    <>
      <Row
        anchor="base-palette"
        label="Base palette"
        description="MUD text uses these 16 colors when you turn off the theme's colors."
      >
        <Button
          disabled={saved === null}
          onClick={() => update({ terminal_base_ansi: null }, { now: true })}
        >
          Reset
        </Button>
      </Row>
      <ColorBlock>
        <ColorGroup
          slots={slots}
          onChange={(key, value) =>
            update({
              terminal_base_ansi: withBaseColor(
                saved,
                ANSI_SLOTS.indexOf(key as (typeof ANSI_SLOTS)[number]),
                value,
              ),
            })
          }
        />
      </ColorBlock>
    </>
  );
}
