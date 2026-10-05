import { PROMPT_SHOW_HELP, PROMPT_SHOW_LABELS, promptShowLock } from '../../prompt/showState';
import { PROMPT_SHOWS, type PromptShow, type PromptShowState } from '../../ipc/prompt';
import { Row, Segmented, type SegmentedOption } from '../../ui';

// Where your prompt shows, from the profile's [prompt] show. It sits
// right after "Draw your own prompt" and saves through the same UiConfig
// bridge, which repaints the prompt on screen and tells every window.
// Nothing can be lifted or pinned while the profile reads no prompt, so
// the row turns off then, with the sentence the Draw row uses in that
// state. The button beside Draw your prompt in Customize prompt offers
// the same places under the same rule.

const OPTIONS: readonly SegmentedOption<PromptShow>[] = PROMPT_SHOWS.map((value) => ({
  value,
  label: PROMPT_SHOW_LABELS[value],
}));

interface PromptShowFieldProps {
  value: PromptShow;
  /** Whether the profile reads a prompt, or null until it is known. */
  state: PromptShowState | null;
  onChange: (show: PromptShow) => void;
}

export function PromptShowField({ value, state, onChange }: PromptShowFieldProps) {
  const { locked, why } = promptShowLock(state);
  const options = locked ? OPTIONS.map((o) => ({ ...o, disabled: true })) : OPTIONS;
  return (
    <Row
      label="Where your prompt shows"
      description={why ?? PROMPT_SHOW_HELP[value]}
      anchor="prompt-show"
      {...(why !== null ? { className: 'is-disabled' } : {})}
    >
      <Segmented options={options} value={value} onChange={onChange} />
    </Row>
  );
}
