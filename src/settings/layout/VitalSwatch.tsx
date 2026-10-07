import { useRef, useState, type CSSProperties } from 'react';
import type { Vital } from '../../ipc/uiConfig';
import { VITAL_LABELS } from '../../panel/vitalsView';
import { ANSI_SLOT_LABELS } from '../../theme/appearanceSettings';
import { ANSI_SLOTS, type AnsiSlot } from '../../theme/baseAnsi';
import type { XtermPalette } from '../../theme/themes';
import { CheckIcon, cx } from '../../ui';
import {
  MenuItem,
  MenuSeparator,
  MenuSurface,
  type MenuCloseReason,
  type MenuPlacement,
} from '../../ui/MenuSurface';
import {
  colorChoices,
  COLOR_MARK_WORDS,
  type ColorChoice,
  type ColorMark,
} from './vitalColorMarks';

// A vital's color under Customize vitals (board 2 of the Vitals Styles
// review, Q4): a 22 pt swatch of the color you picked, or a dashed ring
// for Default. A click opens the list Chat uses for a channel, Default
// and the theme's sixteen, each on its play palette color, with a check
// on your pick and a mark on a color near the low or warn tone.

/** The list's width, and its gap under the swatch and past its right
 *  edge. */
const LIST_WIDTH = 232;
const LIST_GAP = 8;
const LIST_PAST = 6;

/** Where the list opens: under the swatch, its right edge 6 past the
 *  swatch's, and over the swatch at the window's foot. */
function listAt(swatch: DOMRect): MenuPlacement {
  return {
    x: swatch.right + LIST_PAST - LIST_WIDTH,
    y: swatch.bottom + LIST_GAP,
    flipY: swatch.top - LIST_GAP,
  };
}

export function VitalSwatch({
  vital,
  slot,
  palette,
  marks,
  disabled,
  onPick,
}: {
  vital: Vital;
  /** The ANSI slot you picked, or undefined for Default. */
  slot: number | undefined;
  palette: XtermPalette;
  marks: Partial<Record<AnsiSlot, ColorMark>>;
  disabled: boolean;
  /** Saves a slot, or null for Default. */
  onPick: (slot: number | null) => void;
}) {
  const button = useRef<HTMLButtonElement | null>(null);
  const [at, setAt] = useState<MenuPlacement | null>(null);
  const label = VITAL_LABELS[vital];
  const picked = slot === undefined ? null : ANSI_SLOTS[slot];
  const name = picked === null ? 'Default' : ANSI_SLOT_LABELS[picked];
  // Escape and a pick hand the keys back to the swatch. A click
  // elsewhere leaves them where it landed.
  const close = (reason: MenuCloseReason | 'pick') => {
    setAt(null);
    if (reason !== 'outside') button.current?.focus();
  };
  const pick = (next: number | null) => {
    close('pick');
    if (next !== (slot ?? null)) onPick(next);
  };
  const item = (choice: ColorChoice) => (
    <MenuItem
      key={choice.label}
      onSelect={() => pick(choice.slot)}
      trailing={
        choice.mark || choice.checked ? (
          <span className="st-vital-color-end">
            {choice.mark && (
              <span className="st-vital-color-mark">{COLOR_MARK_WORDS[choice.mark]}</span>
            )}
            {choice.checked && <CheckIcon className="pane-menu-check" />}
          </span>
        ) : null
      }
    >
      <span
        className={cx('pane-menu-swatch', choice.swatch === null && 'is-default')}
        style={choice.swatch === null ? undefined : { background: choice.swatch }}
      />
      {choice.label}
    </MenuItem>
  );
  const [fallback, ...slots] = colorChoices(palette, marks, slot);
  return (
    <>
      <button
        ref={button}
        type="button"
        className={cx('st-vital-swatch', picked === null && 'is-default', at && 'is-open')}
        style={picked === null ? undefined : ({ '--swatch': palette[picked] } as CSSProperties)}
        aria-label={`Color for ${label}, ${name}`}
        title={name}
        aria-haspopup="menu"
        aria-expanded={at !== null}
        disabled={disabled}
        onClick={(e) => setAt(at ? null : listAt(e.currentTarget.getBoundingClientRect()))}
      />
      {at && (
        <MenuSurface
          label={`Color for ${label}`}
          className="st-vital-colors"
          anchor={button.current}
          at={at}
          onClose={close}
        >
          {item(fallback)}
          <MenuSeparator />
          {slots.map(item)}
        </MenuSurface>
      )}
    </>
  );
}
