import { useEffect, useId, useState } from 'react';
import { setBaseAnsi } from '../../theme/baseAnsi';
import { renderFontStack } from '../../lib/fontLoader';
import { INPUT_CURSOR_STYLES, type InputCursorStyle } from '../../ipc/uiConfig';
import type { SettingsTarget } from '../../lib/settingsNav';
import { getCurrentThemeId } from '../../theme/theme';
import { findTheme, resolveThemeTerminalColors } from '../../theme/themes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import {
  Card,
  ColorField,
  Disclosure,
  DisclosurePanel,
  NumberField,
  Row,
  Section,
  Segmented,
  Toggle,
  type SegmentedOption,
} from '../../ui';
import { PromptSection } from './InputPrompt';

// Settings, Input (P12). The Command line card holds the caret shape,
// keep last command, chat spell check, the sent command color, and macro
// echo. The Prompt section follows with your game's prompt, Draw your own
// prompt and where it shows, and a preview of your design. Advanced opens
// on paste pacing. Every change saves on its own.

const CARET_NAMES: Record<InputCursorStyle, string> = {
  block: 'Block',
  block_outline: 'Outline',
  half_block: 'Half block',
  underline: 'Underline',
  underline_thick: 'Thick underline',
  pipe: 'Pipe',
  pipe_thick: 'Thick pipe',
};

const CARETS: readonly SegmentedOption<InputCursorStyle>[] = INPUT_CURSOR_STYLES.map((id) => ({
  value: id,
  name: CARET_NAMES[id],
  label: (
    <span className="st-caret" aria-hidden="true">
      <span className={`st-caret-shape st-caret-${id}`} />
    </span>
  ),
}));

// Rows inside Advanced. A deep link or search hit on one opens it.
const ADVANCED_ANCHORS: ReadonlySet<string> = new Set(['paste-delay']);

function opensAdvanced(target: SettingsTarget): boolean {
  return (
    target.section === 'advanced' ||
    (target.anchor !== undefined && ADVANCED_ANCHORS.has(target.anchor))
  );
}

export function InputPage({ target, navSeq, config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const [advanced, setAdvanced] = useState(() => opensAdvanced(target));
  const advancedId = useId();
  const baseAnsi = config?.terminal_base_ansi ?? null;

  // The preview draws with the terminal's colors, the base palette among
  // them while the theme's colors for MUD text are off.
  useEffect(() => {
    setBaseAnsi(baseAnsi);
  }, [baseAnsi]);

  useEffect(() => {
    if (opensAdvanced(target)) setAdvanced(true);
    // navSeq marks each navigation, even to the same target.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navSeq]);

  if (!config) return null;

  // Sent commands draw in the terminal's own text color until you pick
  // one.
  const terminalText = findTheme(getCurrentThemeId()).xterm.foreground;

  return (
    <>
      <Section id="command-line" title="Command line">
        <Row label="Caret shape" anchor="caret">
          <Segmented
            options={CARETS}
            value={config.input_cursor_style}
            onChange={(id) => update({ input_cursor_style: id })}
          />
        </Row>
        <Row
          label="Keep last command"
          description="Your last command stays in the line, selected, so Enter sends it again."
          anchor="keep-last"
        >
          <Toggle
            checked={config.keep_last_command}
            onChange={(on) => update({ keep_last_command: on })}
          />
        </Row>
        <Row
          label="Check spelling when you chat"
          description="Vosh checks only lines that start with say, tell, reply, or a channel name."
          anchor="spellcheck"
        >
          <Toggle
            checked={config.spellcheck_prompt}
            onChange={(on) => update({ spellcheck_prompt: on })}
          />
        </Row>
        <Row
          label="Offer the card when the game’s editor opens"
          description="Type note edit or description edit and Vosh offers to open it in its writing card."
          anchor="writing-offer"
        >
          <Toggle checked={config.writing_offer} onChange={(on) => update({ writing_offer: on })} />
        </Row>
        <Row
          label="Mark your commands"
          description="Draws a grey › before each command you send, except after a prompt that already ends in >."
          anchor="mark-commands"
        >
          <Toggle
            checked={config.input_echo_caret}
            onChange={(on) => update({ input_echo_caret: on })}
          />
        </Row>
        <Row label="Sent command color" anchor="sent-color">
          <ColorField
            value={config.input_echo_color ?? ''}
            onChange={(color) => update({ input_echo_color: color || null })}
            allowEmpty
            // The echo reads only #rrggbb (colorizeEcho in maskedInput.ts).
            hexOnly
            placeholder="Theme default"
            emptySwatch={terminalText}
            pickerLabel="Choose a sent command color"
          />
        </Row>
        <Row label="Show the commands your macros send" anchor="echo-macros">
          <Toggle checked={config.echo_macros} onChange={(on) => update({ echo_macros: on })} />
        </Row>
      </Section>

      <PromptSection
        fontFamily={renderFontStack(config.font_family)}
        themeTerminalColors={resolveThemeTerminalColors(config.theme_terminal_colors)}
        brightBold={config.bright_bold}
        onError={onError}
      />

      <section className="st-section" aria-label="Advanced" data-st-anchor="advanced">
        <Card>
          <Disclosure
            label="Advanced"
            description="Pace long pastes."
            expanded={advanced}
            aria-controls={advanced ? advancedId : undefined}
            onClick={() => setAdvanced((open) => !open)}
          />
          {advanced && (
            <DisclosurePanel id={advancedId}>
              <Row label="Wait between pasted lines" anchor="paste-delay">
                <NumberField
                  value={config.paste_line_delay_ms}
                  onChange={(ms) => update({ paste_line_delay_ms: ms })}
                  min={0}
                  max={10_000}
                  step={50}
                  unit="ms"
                  unitName="milliseconds"
                />
              </Row>
            </DisclosurePanel>
          )}
        </Card>
      </section>
    </>
  );
}
