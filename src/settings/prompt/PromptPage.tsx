import { renderFontStack } from '../../lib/fontLoader';
import { resolveThemeTerminalColors } from '../../theme/themes';
import { PromptSection } from '../input/InputPrompt';
import type { SettingsPageProps } from '../pageTypes';

// Prompt (Settings layout, answered October 8): the Prompt section,
// moved whole from Input with its anchors (Q3). Your game's prompt,
// Draw your own prompt, Where your prompt shows and the preview, saved
// for the profile Settings shows.

export function PromptPage({ config, onError }: SettingsPageProps) {
  if (!config) return null;
  return (
    <PromptSection
      fontFamily={renderFontStack(config.font_family)}
      themeTerminalColors={resolveThemeTerminalColors(config.theme_terminal_colors)}
      brightBold={config.bright_bold}
      onError={onError}
    />
  );
}
