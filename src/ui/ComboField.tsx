import {
  forwardRef,
  useEffect,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import { createPortal } from 'react-dom';
import { scrollWithin } from '../lib/scrollWithin';
import { cx } from './cx';
import { Field, type FieldProps } from './Field';
import { CheckIcon, ChevronDownIcon } from './icons';

export interface ComboFieldProps extends FieldProps {
  /** What the list offers, in the order it shows them. */
  options: readonly string[];
  /** A choice taken from the list, by a click or by Enter on it. Text
   *  you type and keep goes through the field's own keys and blur. */
  onPick: (option: string) => void;
  /** What the button at the end of the field opens, like `Groups`. */
  listLabel: string;
}

/** Where the list sits: under the field, its width, in window pixels. */
interface Spot {
  left: number;
  top: number;
  width: number;
}

const GAP = 4;

/** A settings field that also offers choices, like the Group field,
 *  which lists the groups you have and still takes a new name. It draws
 *  as Field with the Select chevron at its end. ArrowDown or the
 *  chevron opens the list, typing narrows it to the choices that hold
 *  the text, and Enter or a click takes the lit one. Esc closes the
 *  list first and goes to the field's own keys after. The list draws
 *  on the menu recipe under the field. */
export const ComboField = forwardRef<HTMLInputElement, ComboFieldProps>(function ComboField(
  {
    options,
    onPick,
    listLabel,
    value,
    onChange,
    onKeyDown,
    onBlur,
    width = 240,
    className,
    ...rest
  },
  ref,
) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  useImperativeHandle(ref, () => inputRef.current as HTMLInputElement);
  const listId = useId();
  const [open, setOpen] = useState(false);
  // The list narrows to your text only once you type, so opening it on
  // a value shows every choice.
  const [typed, setTyped] = useState(false);
  const [active, setActive] = useState(-1);
  const [spot, setSpot] = useState<Spot | null>(null);

  const query = typed ? value.trim().toLowerCase() : '';
  const shown = query ? options.filter((o) => o.toLowerCase().includes(query)) : options;
  const listOpen = open && shown.length > 0;
  const optionId = (i: number) => `${listId}-${i}`;

  const close = () => {
    setOpen(false);
    setActive(-1);
  };
  const openList = () => {
    setTyped(false);
    setOpen(true);
    setActive(Math.max(0, options.indexOf(value.trim())));
  };
  const pick = (option: string) => {
    close();
    onPick(option);
  };

  useLayoutEffect(() => {
    if (!listOpen) return;
    const place = () => {
      // A field with no layout, as in a test, places no list.
      const r = inputRef.current?.getBoundingClientRect?.();
      if (r) setSpot({ left: r.left, top: r.bottom + GAP, width: r.width });
    };
    place();
    // The field moves with whatever scrolls under it, so the list goes.
    const onScroll = (e: Event) => {
      if (!(e.target instanceof Node && document.getElementById(listId)?.contains(e.target))) {
        close();
      }
    };
    window.addEventListener('resize', place);
    window.addEventListener('scroll', onScroll, true);
    return () => {
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', onScroll, true);
    };
  }, [listOpen, listId]);

  // The lit choice stays in view as the arrows move it.
  useEffect(() => {
    if (!listOpen || active < 0) return;
    scrollWithin(document.getElementById(optionId(active)), { block: 'nearest' });
    // optionId reads only the stable id.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [listOpen, active]);

  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!listOpen) {
        openList();
        return;
      }
      const step = e.key === 'ArrowDown' ? 1 : -1;
      setActive((i) => Math.min(shown.length - 1, Math.max(0, i + step)));
      return;
    }
    if (e.key === 'Enter' && listOpen && active >= 0 && active < shown.length) {
      e.preventDefault();
      pick(shown[active]);
      return;
    }
    if (e.key === 'Escape' && listOpen) {
      e.preventDefault();
      e.stopPropagation();
      close();
      return;
    }
    // Enter on your own text, and Tab, leave the list to the field.
    if (e.key === 'Tab' || e.key === 'Enter') close();
    onKeyDown?.(e);
  };

  return (
    <span className={cx('st-combo', className)} style={{ width }}>
      <Field
        {...rest}
        ref={inputRef}
        width="100%"
        className="st-combo-input"
        value={value}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={listOpen}
        aria-controls={listOpen ? listId : undefined}
        aria-activedescendant={listOpen && active >= 0 ? optionId(active) : undefined}
        onChange={(text) => {
          onChange(text);
          setTyped(true);
          setOpen(true);
          setActive(-1);
        }}
        onKeyDown={keys}
        onBlur={(e) => {
          close();
          onBlur?.(e);
        }}
      />
      <button
        type="button"
        className="st-combo-open"
        tabIndex={-1}
        aria-label={listLabel}
        disabled={rest.disabled || options.length === 0}
        // Keep focus in the field, which the list belongs to.
        onPointerDown={(e) => e.preventDefault()}
        onClick={() => {
          inputRef.current?.focus();
          if (listOpen) close();
          else openList();
        }}
      >
        <ChevronDownIcon size={12} />
      </button>
      {listOpen &&
        spot &&
        createPortal(
          <ul
            id={listId}
            role="listbox"
            aria-label={listLabel}
            className="menu st-combo-list"
            style={{ left: spot.left, top: spot.top, width: spot.width }}
          >
            {shown.map((option, i) => (
              // The row draws as a menu row inside its list item, as
              // MenuItem's button does.
              <li key={option} role="none">
                <div
                  id={optionId(i)}
                  role="option"
                  aria-selected={i === active}
                  className="menu-item st-combo-option"
                  onPointerDown={(e) => e.preventDefault()}
                  onPointerMove={() => setActive(i)}
                  onClick={() => pick(option)}
                >
                  <span className="menu-label">{option}</span>
                  {option === value.trim() && <CheckIcon size={12} className="menu-check" />}
                </div>
              </li>
            ))}
          </ul>,
          document.body,
        )}
    </span>
  );
});
