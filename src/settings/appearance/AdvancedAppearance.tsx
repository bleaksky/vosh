import { useId } from 'react';
import type { UiConfig } from '../../ipc/uiConfig';
import type { UpdateConfig } from '../useSettingsAutoSave';
import { Card, Disclosure, DisclosurePanel, Row, Toggle } from '../../ui';
import { BasePaletteRows } from './BasePaletteRows';
import { CustomThemeRows } from './CustomThemeRows';
import { FontStackRow } from './FontStackRow';

interface AdvancedAppearanceProps {
  config: UiConfig;
  update: UpdateConfig;
  open: boolean;
  onToggle: () => void;
}

/** The quiet Advanced row at the end of Appearance. It holds what the
 *  board leaves out and you still use: custom themes, the base palette,
 *  bold bright text and the font stack. Blinking text lives on
 *  Accessibility, the split divider color on Layout and the sent
 *  command color on Input. */
export function AdvancedAppearance({ config, update, open, onToggle }: AdvancedAppearanceProps) {
  const panelId = useId();
  return (
    <section className="st-section" data-st-anchor="advanced" aria-label="Advanced">
      <Card>
        <Disclosure
          label="Advanced"
          description="Edit custom themes and the base palette."
          expanded={open}
          aria-controls={panelId}
          onClick={onToggle}
        />
        {open && (
          <DisclosurePanel id={panelId}>
            <CustomThemeRows config={config} update={update} />
            <BasePaletteRows config={config} update={update} />
            <Row
              anchor="bright-bold"
              label="Bright text in bold"
              description="On macOS, bright colors draw in the bold weight of your font."
            >
              <Toggle
                checked={config.bright_bold}
                onChange={(on) => update({ bright_bold: on }, { now: true })}
              />
            </Row>
            <FontStackRow config={config} update={update} />
          </DisclosurePanel>
        )}
      </Card>
    </section>
  );
}
