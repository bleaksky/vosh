import { ThemesTab } from '../../ThemesTab';
import { LegacyIsland, TypographyTab } from '../legacy/LegacyEditors';
import type { SettingsPageProps } from '../pageTypes';
import { Section } from '../ui';

// Placeholder for the Appearance board (SettingsAppearance.dc.html).
// One Section per board heading holds the old editor for that area,
// so nothing goes out of reach before the new page lands: the theme
// catalog and custom theme editor under Theme, the old typography tab
// under Terminal text. Replace this whole component with the board.
export function AppearanceGroup({ config, setConfig, onError }: SettingsPageProps) {
  return (
    <>
      <Section id="theme" title="Theme">
        <LegacyIsland>
          <ThemesTab config={config} setConfig={setConfig} onError={onError} />
        </LegacyIsland>
      </Section>
      <Section id="text" title="Terminal text">
        <LegacyIsland>
          <TypographyTab config={config} setConfig={setConfig} onError={onError} />
        </LegacyIsland>
      </Section>
    </>
  );
}
