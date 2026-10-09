import { normalizeScreenReaderBurst, SCREEN_READER_BURSTS } from '../../ipc/uiConfig';
import APP_SHORTCUTS from '../../lib/appShortcuts.json';
import { resolveBlinkText, useReduceMotion } from '../../lib/blink';
import { shortcutKeys } from '../../lib/shortcuts';
import { colorVisionNote } from '../../theme/appearanceSettings';
import { toColorVision } from '../../theme/gameFit';
import { resolveThemeTerminalColors } from '../../theme/themes';
import { Keycap, Row, Section, Select, Toggle } from '../../ui';
import type { SettingsPageProps } from '../pageTypes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';

// Accessibility: the rows that make the game easier to see. Screen
// reader sits on top, and its rows stay live while the reader is off.
// Color and contrast holds Color vision, Fit game colors and Keep
// highlight colors readable, and Motion holds Blinking text, moved whole
// from Appearance with their anchors so old links still land. Every
// moved row saves through the field it saved through on Appearance.

const COLOR_VISIONS = [
  { value: 'typical', label: 'Typical' },
  { value: 'deuteranopia', label: 'Deuteranopia' },
  { value: 'protanopia', label: 'Protanopia' },
  { value: 'tritanopia', label: 'Tritanopia' },
] as const;

const BURSTS = SCREEN_READER_BURSTS.map((n) => ({ value: String(n), label: `${n} lines` }));

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
      <Section id="screen-reader" title="Screen reader">
        <Row
          anchor="read-game-lines"
          label="Read new game lines"
          description="VoiceOver reads each line the game sends, after your gags and routes."
        >
          <Toggle
            checked={config.screen_reader}
            onChange={(on) => update({ screen_reader: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="read-in-background"
          label="Read in the background"
          description="Keep reading while you are in another app."
        >
          <Toggle
            checked={config.screen_reader_background}
            onChange={(on) => update({ screen_reader_background: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="read-your-prompt"
          label="Read your prompt"
          description={
            <>
              Your prompt comes every pulse. Off reads it only when you press{' '}
              <span className="keys st-keys-inline">
                {shortcutKeys(APP_SHORTCUTS['read-prompt']).map((key) => (
                  <Keycap key={key}>{key}</Keycap>
                ))}
              </span>
            </>
          }
        >
          <Toggle
            checked={config.screen_reader_prompt}
            onChange={(on) => update({ screen_reader_prompt: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="long-bursts"
          label="Long bursts"
          description="When more lines than this land at once, VoiceOver reads how many came and the last one."
        >
          <Select
            value={String(config.screen_reader_burst)}
            options={BURSTS}
            onChange={(n) =>
              update({ screen_reader_burst: normalizeScreenReaderBurst(Number(n)) }, { now: true })
            }
          />
        </Row>
      </Section>
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
