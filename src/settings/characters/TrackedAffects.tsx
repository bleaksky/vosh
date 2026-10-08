import { useEffect, useId, useRef, useState } from 'react';
import { normalizeAffectName } from '../../lib/affects';
import { useEscape } from '../../lib/escapeStack';
import type { TrackedAffect } from '../../ipc/affects';
import { useAffects } from '../../stores/gmcp/affectsStore';
import {
  addTrackedAffect,
  affectSuggestions,
  removeTrackedAffect,
  trackedAffectLabel,
} from './trackedAffectEdit';
import { Card, Chip, ChipButton, PlusIcon, Section } from '../../ui';

// Tracked affects on the Characters board: a quiet line, then a chip
// per affect the profile tracks with a close button that stops
// tracking it, then Add affect. Add affect turns into a field in place
// with the affects on you now as suggestions. Return adds, Esc puts
// the chip back.

interface Props {
  tracked: TrackedAffect[];
  /** Replace the list. The page saves it to the selected profile. */
  onEdit: (edit: (list: TrackedAffect[]) => TrackedAffect[]) => void;
}

export function TrackedAffects({ tracked, onEdit }: Props) {
  const [adding, setAdding] = useState(false);
  // After a chip goes, focus the close button that took its place, or
  // Add affect when it was the last.
  const [focusAt, setFocusAt] = useState<number | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);
  const addRef = useRef<HTMLButtonElement | null>(null);
  const wasAdding = useRef(false);
  const refocus = useRef(false);

  useEffect(() => {
    if (focusAt === null) return;
    const closes = listRef.current?.querySelectorAll<HTMLButtonElement>('.st-chip-close');
    const next = closes?.[Math.min(focusAt, closes.length - 1)];
    (next ?? addRef.current)?.focus();
    setFocusAt(null);
  }, [focusAt, tracked]);

  // Esc hands focus back to Add affect. Leaving the field leaves focus
  // on whatever you clicked or tabbed to.
  useEffect(() => {
    if (wasAdding.current && !adding && refocus.current) addRef.current?.focus();
    refocus.current = false;
    wasAdding.current = adding;
  }, [adding]);

  const closeField = (returnFocus: boolean) => {
    refocus.current = returnFocus;
    setAdding(false);
  };

  return (
    <Section id="tracked" title="Tracked affects" card={false}>
      <Card className="st-tracked">
        <p className="st-tracked-note">
          The Affects pane marks these and shows any you are missing.
        </p>
        <ul ref={listRef} className="st-chips">
          {tracked.map((entry, index) => {
            const label = trackedAffectLabel(entry);
            const key = normalizeAffectName(entry.name);
            return (
              <Chip
                key={key}
                as="li"
                removeLabel={`Stop tracking ${label}`}
                onRemove={() => {
                  onEdit((list) =>
                    removeTrackedAffect(
                      list,
                      list.findIndex((e) => normalizeAffectName(e.name) === key),
                    ),
                  );
                  setFocusAt(index);
                }}
              >
                {label}
              </Chip>
            );
          })}
          <li className="st-chip-slot">
            {adding ? (
              <AddAffectField
                tracked={tracked}
                onAdd={(name) => onEdit((list) => addTrackedAffect(list, name))}
                onClose={closeField}
              />
            ) : (
              <ChipButton
                ref={addRef}
                icon={<PlusIcon size={12} />}
                data-st-anchor="add-affect"
                data-st-coach="Pick Add affect… and name a spell you keep up."
                onClick={() => setAdding(true)}
              >
                Add affect…
              </ChipButton>
            )}
          </li>
        </ul>
      </Card>
    </Section>
  );
}

interface AddAffectFieldProps {
  tracked: TrackedAffect[];
  onAdd: (name: string) => void;
  /** `returnFocus` is true for Esc pressed in the field. Leaving the
   *  field empty closes it with focus left where you moved it. */
  onClose: (returnFocus: boolean) => void;
}

/** The field Add affect turns into: a combobox over the affects on you
 *  that the profile does not track yet. */
function AddAffectField({ tracked, onAdd, onClose }: AddAffectFieldProps) {
  const current = useAffects();
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(-1);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listId = useId();
  const suggestions = affectSuggestions(current, tracked, query);
  const open = suggestions.length > 0;
  const optionId = (i: number) => `${listId}-${i}`;

  useEffect(() => inputRef.current?.focus(), []);
  useEffect(() => setActive(-1), [query]);

  useEscape(true, () => onClose(document.activeElement === inputRef.current));

  const add = (name: string) => {
    if (name.trim().length === 0) return;
    onAdd(name);
    setQuery('');
    setActive(-1);
  };

  return (
    <span className="st-affect-add">
      <input
        ref={inputRef}
        className="st-chip-input"
        type="text"
        role="combobox"
        aria-label="Affect to track"
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listId}
        aria-activedescendant={open && active >= 0 ? optionId(active) : undefined}
        placeholder="Affect name"
        spellCheck={false}
        autoComplete="off"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' && open) {
            e.preventDefault();
            setActive((i) => Math.min(i + 1, suggestions.length - 1));
          } else if (e.key === 'ArrowUp' && open) {
            e.preventDefault();
            setActive((i) => Math.max(i - 1, -1));
          } else if (e.key === 'Enter') {
            e.preventDefault();
            const picked = active >= 0 ? suggestions[active] : undefined;
            add(picked ? picked.name : query);
          }
        }}
        onBlur={() => {
          if (query.trim().length === 0) onClose(false);
        }}
      />
      <ul
        id={listId}
        role="listbox"
        aria-label="Affects on you"
        className="st-suggest"
        hidden={!open}
      >
        {suggestions.map((s, i) => (
          <li
            key={s.name}
            id={optionId(i)}
            role="option"
            aria-selected={i === active}
            className="st-suggest-item"
            // Keep focus in the field while you pick with the pointer.
            onMouseDown={(e) => e.preventDefault()}
            onMouseMove={() => setActive(i)}
            onClick={() => add(s.name)}
          >
            {s.name}
          </li>
        ))}
      </ul>
    </span>
  );
}
