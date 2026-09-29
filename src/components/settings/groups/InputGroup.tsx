import { useEffect, useId, useState } from 'react';
import { promptPreviewChunks } from '../../../lib/promptPreview';
import { styleToCss } from '../../../lib/ansi';
import { INPUT_CURSOR_STYLES, type InputCursorStyle } from '../../../lib/session';
import type { SettingsTarget } from '../../../lib/settingsNav';
import { useVitals } from '../../../lib/stores/vitalsStore';
import { getCurrentThemeId } from '../../../lib/theme';
import { findTheme } from '../../../lib/themes';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import {
  Card,
  ColorField,
  Disclosure,
  DisclosurePanel,
  Field,
  NumberField,
  Row,
  Section,
  Segmented,
  Toggle,
  type SegmentedOption,
} from '../ui';

// Settings, Input (SettingsInput.dc.html). The Command line card holds
// the caret shape, keep last command, chat spell check, the sent
// command color, and macro echo. The Advanced card opens on paste
// pacing and your own prompt, with a live preview of the template.
// Every change saves on its own.

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
const ADVANCED_ANCHORS: ReadonlySet<string> = new Set(['paste-delay', 'prompt']);

function opensAdvanced(target: SettingsTarget): boolean {
  return (
    target.section === 'advanced' ||
    (target.anchor !== undefined && ADVANCED_ANCHORS.has(target.anchor))
  );
}

export function InputGroup({ target, navSeq, config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const [advanced, setAdvanced] = useState(() => opensAdvanced(target));
  const advancedId = useId();

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
        <Row label="Sent command color" anchor="sent-color">
          <ColorField
            value={config.input_echo_color ?? ''}
            onChange={(color) => update({ input_echo_color: color || null })}
            allowEmpty
            // The echo reads only #rrggbb (colorizeEcho in Input.tsx).
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

      <section className="st-section" aria-label="Advanced" data-st-anchor="advanced">
        <Card>
          <Disclosure
            label="Advanced"
            description="Pace long pastes and draw your own prompt."
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
              <PromptBlock
                enabled={config.prompt_template_enabled}
                template={config.prompt_template}
                textColor={terminalText}
                onEnabled={(on) => update({ prompt_template_enabled: on })}
                onTemplate={(text) => update({ prompt_template: text })}
              />
            </DisclosurePanel>
          )}
        </Card>
      </section>
    </>
  );
}

/** Draw your own prompt: the toggle on the label line, then the
 *  template in the terminal font, then the template drawn the way the
 *  terminal would with your vitals full. */
function PromptBlock({
  enabled,
  template,
  textColor,
  onEnabled,
  onTemplate,
}: {
  enabled: boolean;
  template: string;
  textColor: string;
  onEnabled: (on: boolean) => void;
  onTemplate: (text: string) => void;
}) {
  const fieldId = useId();
  const previewId = useId();
  const vitals = useVitals();
  const chunks = promptPreviewChunks(template, vitals);
  return (
    <div className="st-block" data-st-anchor="prompt" data-st-flash="">
      <Row
        label="Draw your own prompt"
        description="It takes the place of your MUD's prompt. Capture the prompt with #prompt first."
      >
        <Toggle checked={enabled} onChange={onEnabled} />
      </Row>
      <label htmlFor={fieldId} className="st-visually-hidden">
        Prompt template
      </label>
      <Field
        id={fieldId}
        mono
        width="100%"
        className="st-prompt-template"
        value={template}
        placeholder="[%hp_bar:10 %hp/%maxhp hp] > "
        aria-describedby={previewId}
        onChange={onTemplate}
      />
      <output
        id={previewId}
        htmlFor={fieldId}
        aria-label="Preview"
        className="st-prompt-preview"
        style={{ color: textColor }}
      >
        {chunks.map((chunk, i) => (
          <span key={i} style={styleToCss(chunk.style)}>
            {chunk.text}
          </span>
        ))}
      </output>
    </div>
  );
}
