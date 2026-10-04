import { PROMPT_SHOW_LABELS, promptShowLock, type PromptShowState } from '../../lib/promptShow';
import { PROMPT_SHOWS, type PromptShow } from '../../lib/session';
import { MenuButton, type MenuChoice } from './MenuButton';

// Where your prompt shows, at the foot of Customize prompt beside Draw
// your prompt. A compact menu button (MenuButton) reads the place your
// prompt shows now and opens a menu of the three places. A pick saves to
// the profile's [prompt] show, the field the Settings row writes, and
// the card follows your prompt there. While the profile reads no prompt
// the button turns off and says why, with the sentence of the Settings
// row.

/** The button's name and the menu's. */
const NAME = 'Where your prompt shows';

const CHOICES: readonly MenuChoice<PromptShow>[] = PROMPT_SHOWS.map((place) => ({
  value: place,
  label: PROMPT_SHOW_LABELS[place],
}));

interface ShowButtonProps {
  /** The place the card's table holds. */
  value: PromptShow;
  /** Whether the profile reads a prompt, or null until it is known. */
  state: PromptShowState | null;
  onChange: (show: PromptShow) => void;
}

export function ShowButton({ value, state, onChange }: ShowButtonProps) {
  const { locked, why } = promptShowLock(state);
  return (
    <MenuButton
      name={NAME}
      choices={CHOICES}
      value={value}
      place="above-start"
      locked={locked}
      why={why}
      onChange={onChange}
    />
  );
}
