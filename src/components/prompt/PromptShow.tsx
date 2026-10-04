import { useEffect, useId, useState } from 'react';
import { PROMPT_SHOW_LABELS, promptShowLock, type PromptShowState } from '../../lib/promptShow';
import { PROMPT_SHOWS, type PromptShow } from '../../lib/session';
import { Button, CheckIcon, ChevronDownIcon } from '../settings/ui';
import { CardMenu } from './CardMenu';

// Where your prompt shows, at the foot of Customize prompt beside Draw
// your prompt. A compact button reads the place your prompt shows now
// with a chevron and opens a menu of the three places, the current one
// checked on the right as the pane menus check theirs. A pick saves to
// the profile's [prompt] show, the field the Settings row writes, and
// the card follows your prompt there. While the profile reads no prompt
// the button turns off and says why, with the sentence of the Settings
// row. It stays where Tab reaches it, so a screen reader hears why too.
//
// Enter and Space press it as they press any button, the menu's arrow
// keys move between the places, and Esc or Tab closes it (CardMenu).

/** The button's name and the menu's. */
const NAME = 'Where your prompt shows';

/** The menu's width, the narrow pane menus' width. */
export const SHOW_MENU_WIDTH = 160;

interface ShowButtonProps {
  /** The place the card's table holds. */
  value: PromptShow;
  /** Whether the profile reads a prompt, or null until it is known. */
  state: PromptShowState | null;
  onChange: (show: PromptShow) => void;
}

export function ShowButton({ value, state, onChange }: ShowButtonProps) {
  const whyId = useId();
  const [menuAt, setMenuAt] = useState<HTMLElement | null>(null);
  const { locked, why } = promptShowLock(state);
  const label = PROMPT_SHOW_LABELS[value];

  // The lock can land while the menu is open, as the state reads again.
  useEffect(() => {
    if (locked) setMenuAt(null);
  }, [locked]);
  const openAt = locked ? null : menuAt;

  const choose = (place: PromptShow) => {
    // Focus goes back to the button before the menu goes, so the card
    // keeps it on the button and not on itself.
    openAt?.focus({ preventScroll: true });
    setMenuAt(null);
    if (place !== value) onChange(place);
  };

  return (
    <>
      <Button
        className="pc-show-button"
        aria-label={`${NAME}, ${label}`}
        aria-haspopup="menu"
        aria-expanded={openAt !== null}
        aria-disabled={locked || undefined}
        aria-describedby={why === null ? undefined : whyId}
        title={why ?? undefined}
        onClick={(e) => {
          if (locked) return;
          setMenuAt(openAt ? null : e.currentTarget);
        }}
      >
        <span>{label}</span>
        <ChevronDownIcon />
      </Button>
      {why !== null && (
        <span id={whyId} className="st-visually-hidden">
          {why}
        </span>
      )}
      {openAt && (
        <CardMenu
          anchor={openAt}
          place="above-start"
          width={SHOW_MENU_WIDTH}
          label={NAME}
          onClose={() => setMenuAt(null)}
        >
          {PROMPT_SHOWS.map((place) => {
            const checked = place === value;
            return (
              <li key={place} role="none">
                <button
                  type="button"
                  role="menuitemradio"
                  aria-checked={checked}
                  className="ov-menu-item"
                  onClick={() => choose(place)}
                >
                  <span className="ov-menu-label">{PROMPT_SHOW_LABELS[place]}</span>
                  {checked && <CheckIcon className="pane-menu-check" />}
                </button>
              </li>
            );
          })}
        </CardMenu>
      )}
    </>
  );
}
