import {
  PROMPT_SHOW_HELP,
  promptShowDisabledHelp,
  type PromptShowState,
} from '../../../lib/promptShow';
import type { PromptShow } from '../../../lib/session';
import { Row, Segmented, type SegmentedOption } from '../ui';

// Where your prompt shows, from the profile's [prompt] show. It sits
// right after "Draw your own prompt" and saves through the same UiConfig
// bridge, which repaints the prompt on screen and tells every window.
// Nothing can be lifted or pinned while the profile reads no prompt, so
// the row turns off then, with the sentence the Draw row uses in that
// state.

const OPTIONS: readonly SegmentedOption<PromptShow>[] = [
  { value: 'text', label: 'In the text' },
  { value: 'lifted', label: 'Lifted' },
  { value: 'pinned', label: 'Pinned' },
];

interface PromptShowFieldProps {
  value: PromptShow;
  /** Whether the profile reads a prompt, or null until it is known. */
  state: PromptShowState | null;
  onChange: (show: PromptShow) => void;
}

export function PromptShowField({ value, state, onChange }: PromptShowFieldProps) {
  const off = state !== null && !state.capture;
  const options = off || state === null ? OPTIONS.map((o) => ({ ...o, disabled: true })) : OPTIONS;
  return (
    <Row
      label="Where your prompt shows"
      description={off ? promptShowDisabledHelp(state.gameSent) : PROMPT_SHOW_HELP[value]}
      anchor="prompt-show"
      {...(off ? { className: 'is-disabled' } : {})}
    >
      <Segmented options={options} value={value} onChange={onChange} />
    </Row>
  );
}
