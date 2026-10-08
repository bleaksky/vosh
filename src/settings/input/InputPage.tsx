import { useEffect, useId, useState } from 'react';
import { setBaseAnsi } from '../../theme/baseAnsi';
import {
  ECHO_MARK_TEXT_MAX,
  INPUT_CURSOR_STYLES,
  type InputCursorStyle,
  type InputEchoMark,
} from '../../ipc/uiConfig';
import type { SettingsTarget } from '../../lib/settingsNav';
import { getCurrentThemeId } from '../../theme/theme';
import { findTheme } from '../../theme/themes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import {
  Card,
  ColorField,
  Disclosure,
  Field,
  DisclosurePanel,
  NumberField,
  Row,
  Section,
  Segmented,
  Toggle,
  type SegmentedOption,
} from '../../ui';

// Settings, Input. Sent commands holds how your commands echo in the
// scrollback: the mark before them, its color, the command color, dim,
// the mark at the start of the command line, and macro echo. Command
// line holds the caret shape, keep last command and chat spell check.
// Writing card follows with the two rows for the card that opens for
// note edit and description edit. Advanced opens on paste pacing. The
// Prompt section has a tab of its own (PromptPage.tsx). Every change
// saves on its own.

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

const MARKS: readonly SegmentedOption<InputEchoMark>[] = [
  { value: 'off', label: 'Off' },
  { value: 'chevron', label: '›', name: 'Chevron' },
  { value: 'gt', label: '>', name: 'Greater than' },
  { value: 'own', label: 'Your own' },
];

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
  // one, and the mark in the theme's bright black.
  const { foreground: terminalText, brightBlack: markGrey } = findTheme(getCurrentThemeId()).xterm;

  return (
    <>
      <Section id="sent" title="Sent commands">
        <Row
          label="Mark before your commands"
          description="Vosh leaves it out after a prompt that already ends in >."
          anchor="mark-commands"
        >
          <Segmented
            options={MARKS}
            value={config.input_echo_mark}
            onChange={(mark) => update({ input_echo_mark: mark })}
          />
          {config.input_echo_mark === 'own' && (
            <Field
              value={config.input_echo_mark_text}
              onChange={(text) =>
                update({ input_echo_mark_text: [...text].slice(0, ECHO_MARK_TEXT_MAX).join('') })
              }
              width={64}
              mono
              aria-label="Your own mark"
            />
          )}
        </Row>
        <Row label="Mark color" anchor="mark-color">
          <ColorField
            value={config.input_echo_mark_color ?? ''}
            onChange={(color) => update({ input_echo_mark_color: color || null })}
            allowEmpty
            // The echo reads only #rrggbb (echoRgb in maskedInput.ts).
            hexOnly
            placeholder="Theme default"
            emptySwatch={markGrey}
            pickerLabel="Choose a mark color"
          />
        </Row>
        <Row label="Command color" anchor="sent-color">
          <ColorField
            value={config.input_echo_color ?? ''}
            onChange={(color) => update({ input_echo_color: color || null })}
            allowEmpty
            hexOnly
            placeholder="Theme default"
            emptySwatch={terminalText}
            pickerLabel="Choose a command color"
          />
        </Row>
        <Row
          label="Dim sent commands"
          description="Your commands draw faint, so the game’s lines stand out."
          anchor="sent-dim"
        >
          <Toggle
            checked={config.input_echo_dim}
            onChange={(on) => update({ input_echo_dim: on })}
          />
        </Row>
        <Row
          label="Use the same mark in the command line"
          description="The line you type in starts with your mark."
          anchor="mark-line"
        >
          <Toggle
            checked={config.input_line_mark}
            onChange={(on) => update({ input_line_mark: on })}
          />
        </Row>
        <Row label="Show the commands your macros send" anchor="echo-macros">
          <Toggle checked={config.echo_macros} onChange={(on) => update({ echo_macros: on })} />
        </Row>
      </Section>

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
      </Section>

      <Section id="writing" title="Writing card">
        <Row
          label="Offer the card when the game’s editor opens"
          description="Type note edit or description edit and Vosh offers to open it in its writing card."
          anchor="writing-offer"
        >
          <Toggle checked={config.writing_offer} onChange={(on) => update({ writing_offer: on })} />
        </Row>
        <Row
          label="Ask before you post"
          description="Turn this off and Post posts your note at once, unless a report would record a room other than the one you began it in."
          anchor="writing-ask-post"
        >
          <Toggle
            checked={config.writing_ask_post}
            onChange={(on) => update({ writing_ask_post: on })}
          />
        </Row>
      </Section>

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
