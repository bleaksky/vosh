import { useEffect, useId, useState } from 'react';
import type { MenuPlace } from './cardRules';
import { Button, CheckIcon, ChevronDownIcon, VisuallyHidden } from '../ui';
import { MenuItem } from '../ui/MenuSurface';
import { CardMenu } from './CardMenu';

// A compact button at the foot of Customize prompt that reads the
// current choice with a chevron and opens a card menu of the choices,
// the current one checked on the right as the pane menus check theirs.
// Where your prompt shows and the preview both use it. A screen reader
// hears its name and the current choice.
//
// Enter and Space press it as they press any button, the menu's arrow
// keys move between the choices, and Esc or Tab closes it (CardMenu).
// A button that is off stays where Tab reaches it, so a screen reader
// hears why it is off too.

export interface MenuChoice<T extends string> {
  value: T;
  label: string;
}

interface MenuButtonProps<T extends string> {
  /** The button's name and the menu's. */
  name: string;
  /** What the button reads before the current choice, as Preview: does.
   *  Without it the button reads the choice alone. */
  lead?: string;
  choices: readonly MenuChoice<T>[];
  value: T;
  place: MenuPlace;
  /** The button is off and opens no menu. */
  locked?: boolean;
  /** Why it is off, as its tooltip and its description. */
  why?: string | null;
  onChange: (value: T) => void;
}

export function MenuButton<T extends string>({
  name,
  lead,
  choices,
  value,
  place,
  locked = false,
  why = null,
  onChange,
}: MenuButtonProps<T>) {
  const whyId = useId();
  const [menuAt, setMenuAt] = useState<HTMLElement | null>(null);
  const label = choices.find((c) => c.value === value)?.label ?? '';

  // The lock can land while the menu is open.
  useEffect(() => {
    if (locked) setMenuAt(null);
  }, [locked]);
  const openAt = locked ? null : menuAt;

  const choose = (choice: T) => {
    // Focus goes back to the button before the menu goes, so the card
    // keeps it on the button and not on itself.
    openAt?.focus({ preventScroll: true });
    setMenuAt(null);
    if (choice !== value) onChange(choice);
  };

  return (
    <>
      <Button
        className="pc-menu-button"
        aria-label={`${name}, ${label}`}
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
        <span>
          {lead !== undefined && <span className="pc-menu-button-lead">{`${lead} `}</span>}
          {label}
        </span>
        <ChevronDownIcon />
      </Button>
      {why !== null && <VisuallyHidden id={whyId}>{why}</VisuallyHidden>}
      {openAt && (
        <CardMenu anchor={openAt} place={place} label={name} onClose={() => setMenuAt(null)}>
          {choices.map((choice) => {
            const checked = choice.value === value;
            return (
              <MenuItem
                key={choice.value}
                radio
                checked={checked}
                trailing={checked && <CheckIcon className="menu-check" />}
                onSelect={() => choose(choice.value)}
              >
                {choice.label}
              </MenuItem>
            );
          })}
        </CardMenu>
      )}
    </>
  );
}
