import { resolveBlinkText, useReduceMotion } from '../../lib/blink';
import { colorVisionNote } from '../../theme/appearanceSettings';
import { toColorVision } from '../../theme/gameFit';
import { resolveThemeTerminalColors } from '../../theme/themes';
import { Row, Section, Select, Toggle } from '../../ui';
import type { SettingsPageProps } from '../pageTypes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';

// Accessibility: the rows that make the game easier to see, moved whole
// from Appearance with their anchors so old links still land. Color and
// contrast holds Color vision, Fit game colors and Keep highlight colors
// readable. Motion holds Blinking text. Every row saves through the field it saved through on
// Appearance.

const COLOR_VISIONS = [
  { value: 'typical', label: 'Typical' },
  { value: 'deuteranopia', label: 'Deuteranopia' },
  { value: 'protanopia', label: 'Protanopia' },
  { value: 'tritanopia', label: 'Tritanopia' },
] as const;

/** What Blinking text does, under its row. */
const BLINK_TEXT_DESCRIPTION =
  'Text your MUD or prompt sets to blink flashes. It starts off if your system reduces motion.';

export function AccessibilityPage({ config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const reduceMotion = useReduceMotion();
  if (!config) return null;
  const visionNote = colorVisionNote(
    config.color_vision,
    resolveThemeTerminalColors(config.theme_terminal_colors),
  );
  return (
    <>
      <Section id="color" title="Color and contrast">
        <Row
          anchor="color-vision"
          label="Color vision"
          description={
            <>
              Vosh swaps the colors your eyes confuse for colors they tell apart, the way color
              blind modes in games do.
              {visionNote !== '' && (
                <>
                  <br />
                  {visionNote}
                </>
              )}
            </>
          }
        >
          <Select
            value={config.color_vision}
            options={COLOR_VISIONS}
            onChange={(vision) => update({ color_vision: toColorVision(vision) }, { now: true })}
          />
        </Row>
        <Row
          anchor="fit-game-colors"
          label="Fit game colors"
          description="While you play, Vosh lifts the game colors that fade on the theme, and Settings keeps the theme as published."
        >
          <Toggle
            checked={config.fit_game_colors}
            onChange={(on) => update({ fit_game_colors: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="readable-highlights"
          label="Keep highlight colors readable"
          description="Vosh darkens or lightens a color your triggers set when the theme would make it faint."
        >
          <Toggle
            checked={config.readable_highlights}
            onChange={(on) => update({ readable_highlights: on }, { now: true })}
          />
        </Row>
      </Section>
      <Section id="motion" title="Motion">
        <Row anchor="blink-text" label="Blinking text" description={BLINK_TEXT_DESCRIPTION}>
          <Toggle
            checked={resolveBlinkText(config.blink_text, reduceMotion)}
            onChange={(on) => update({ blink_text: on }, { now: true })}
          />
        </Row>
      </Section>
    </>
  );
}
