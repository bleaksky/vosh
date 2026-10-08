import { useEffect, useMemo, useRef, useState } from 'react';
import {
  buildAliasEntries,
  buildPaletteEntries,
  filterEntries,
  initialSelection,
  paletteSections,
  readRecent,
  recordRecent,
  type PaletteDeps,
  type PaletteEntry,
  type PaletteSectionView,
} from './palette';
import { ariaKeyshortcuts, shortcutKeys } from '../../lib/shortcuts';
import { scrollWithin } from '../../lib/scrollWithin';

interface Props {
  deps: PaletteDeps;
  onClose: () => void;
}

/** A submenu level: the rows it lists and its section header. */
interface Level {
  label: string;
  entries: PaletteEntry[];
}

const ICON = {
  width: 16,
  height: 16,
  viewBox: '0 0 16 16',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.25,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
  'aria-hidden': true,
} as const;

/** Keycaps for a shortcut spec, in the platform's glyphs. The row
 *  names the keys in aria-keyshortcuts, so the caps stay out of its
 *  name. */
function Keycaps({ spec }: { spec: string }) {
  return (
    <kbd className="ov-keys" aria-hidden="true">
      {shortcutKeys(spec).map((key, i) => (
        <kbd key={i} className={`ov-key${key.length > 1 ? ' is-wide' : ''}`}>
          {key}
        </kbd>
      ))}
    </kbd>
  );
}

// The ⌘K command palette: 560 wide, centered on the window with its top
// at 15% of the window height, no scrim. Sections with caps headers
// (Recent, View, Session), check marks on toggles that are on, keycaps
// on the right, and a submenu row that opens its list in place.
// Disconnect is the last row and the palette never opens with a
// destructive row selected, so ⌘K then Enter cannot drop the session.
// Entries rebuild on every open so checks track live state. Aliases
// stream in from the backend and, like the pane toggles, find, help,
// and the settings rows, show once you type.
export function CommandPalette({ deps, onClose }: Props) {
  const [query, setQuery] = useState('');
  const [levels, setLevels] = useState<Level[]>([]);
  const [aliasEntries, setAliasEntries] = useState<PaletteEntry[]>([]);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const baseEntries = useMemo(() => buildPaletteEntries(deps), [deps]);
  const recent = useMemo(readRecent, []);

  useEffect(() => {
    // Hand focus back when the palette goes away with nothing else
    // taking it, so the command line and its macros keep working.
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    inputRef.current?.focus();
    let cancelled = false;
    void buildAliasEntries(deps).then((rows) => {
      if (!cancelled) setAliasEntries(rows);
    });
    return () => {
      cancelled = true;
      const active = document.activeElement;
      if (!active || active === document.body) previous?.focus();
    };
    // deps is stable for the lifetime of one palette open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Close on a press anywhere outside the palette. There is no scrim,
  // so the terminal and panel stay visible and clickable behind it. The
  // title band's Search commands button toggles the palette itself, so
  // a press there is left to its click, which would otherwise reopen
  // the palette this press just closed.
  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      if (e.target instanceof Node && rootRef.current?.contains(e.target)) return;
      if (e.target instanceof Element && e.target.closest('[data-palette-anchor]')) return;
      onClose();
    };
    document.addEventListener('pointerdown', onPointer, true);
    return () => document.removeEventListener('pointerdown', onPointer, true);
  }, [onClose]);

  const level = levels.length > 0 ? levels[levels.length - 1] : null;

  const sections = useMemo<PaletteSectionView[]>(() => {
    if (level) {
      const rows = filterEntries(level.entries, query);
      return rows.length > 0 ? [{ key: 'view', label: level.label, rows }] : [];
    }
    return paletteSections([...baseEntries, ...aliasEntries], query, recent);
  }, [level, baseEntries, aliasEntries, query, recent]);

  const rows = useMemo(() => sections.flatMap((s) => s.rows), [sections]);
  const [selected, setSelected] = useState(() => initialSelection(rows, query));

  useEffect(() => {
    // A submenu opens on its checked row (the current theme).
    const checked = level && query.length === 0 ? rows.findIndex((e) => e.checked) : -1;
    setSelected(checked >= 0 ? checked : initialSelection(rows, query));
    // Reset only when the list itself changes shape, not on every
    // render of the same rows.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, levels, aliasEntries.length]);

  // Keep the selected row in view during keyboard navigation.
  useEffect(() => {
    const row = listRef.current?.querySelector('[aria-selected="true"]');
    scrollWithin(row, { block: 'nearest' });
  }, [selected]);

  const openLevel = (entry: PaletteEntry) => {
    if (!entry.children) return;
    setLevels((ls) => [
      ...ls,
      { label: entry.childLabel ?? entry.title, entries: entry.children!() },
    ]);
    setQuery('');
  };

  const closeLevel = () => {
    setLevels((ls) => ls.slice(0, -1));
    setQuery('');
  };

  const activate = (entry: PaletteEntry | undefined) => {
    if (!entry) return;
    if (entry.children) {
      openLevel(entry);
      return;
    }
    if (!level) recordRecent(entry);
    onClose();
    void entry.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const current = selected >= 0 ? rows[selected] : undefined;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (level) closeLevel();
      else onClose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((s) => Math.min(s + 1, rows.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((s) => Math.max(s - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      activate(current);
    } else if (e.key === 'ArrowRight' && current?.children) {
      e.preventDefault();
      openLevel(current);
    } else if ((e.key === 'ArrowLeft' || e.key === 'Backspace') && level && query.length === 0) {
      e.preventDefault();
      closeLevel();
    } else if (e.key === 'Tab') {
      // Focus stays in the search field.
      e.preventDefault();
    }
  };

  const optionId = (i: number) => `ov-pal-opt-${i}`;
  let flatIndex = -1;

  return (
    <div
      ref={rootRef}
      className="ov-palette"
      role="dialog"
      aria-label="Command palette"
      onKeyDown={onKeyDown}
      // Keep the window's click-to-type handler from pulling focus out
      // of the search field when you press a header or the padding.
      onMouseUp={(e) => e.stopPropagation()}
    >
      <div className="ov-pal-search">
        <svg {...ICON} className="ov-pal-search-icon">
          <circle cx="7" cy="7" r="4.5" />
          <path d="M10.5 10.5l3.5 3.5" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          value={query}
          placeholder={level ? `Search ${level.label.toLowerCase()}…` : 'Search commands…'}
          spellCheck={false}
          autoComplete="off"
          autoCapitalize="off"
          autoCorrect="off"
          role="combobox"
          aria-label="Search commands"
          aria-expanded="true"
          aria-controls="ov-pal-list"
          aria-autocomplete="list"
          aria-activedescendant={selected >= 0 ? optionId(selected) : undefined}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="ov-pal-divider" aria-hidden="true" />
      <div
        className="ov-pal-list"
        id="ov-pal-list"
        role="listbox"
        aria-label="Commands"
        ref={listRef}
      >
        {rows.length === 0 && <div className="ov-pal-empty">No commands match</div>}
        {sections.map((section) => (
          <div
            key={section.key}
            className="ov-pal-group"
            role="group"
            aria-labelledby={`ov-pal-head-${section.key}`}
          >
            <div className="ov-pal-head" id={`ov-pal-head-${section.key}`}>
              {section.label}
              {section.key === 'session' && deps.connected && deps.host && (
                <span className="ov-pal-chip">{deps.host}</span>
              )}
            </div>
            {section.rows.map((entry) => {
              flatIndex += 1;
              const idx = flatIndex;
              const isSel = idx === selected;
              return (
                <div key={`${section.key}-${entry.id}`} className="ov-pal-slot" role="none">
                  <button
                    type="button"
                    role="option"
                    id={optionId(idx)}
                    tabIndex={-1}
                    aria-selected={isSel}
                    aria-haspopup={entry.children ? 'true' : undefined}
                    aria-keyshortcuts={entry.keys && ariaKeyshortcuts(entry.keys)}
                    className={`ov-pal-row${isSel ? ' is-selected' : ''}${
                      entry.destructive ? ' is-danger' : ''
                    }`}
                    onPointerMove={() => {
                      if (!isSel) setSelected(idx);
                    }}
                    onClick={() => activate(entry)}
                  >
                    {entry.checked && (
                      <svg {...ICON} className="ov-pal-check">
                        <path d="M3.5 8.5l3 3 6-7" />
                      </svg>
                    )}
                    <span className="ov-pal-title">{entry.title}</span>
                    {entry.meta && (
                      <span className={`ov-pal-meta${entry.metaMono ? ' is-mono' : ''}`}>
                        {entry.meta}
                      </span>
                    )}
                    {entry.children && (
                      <svg {...ICON} className="ov-pal-chevron">
                        <path d="M6.25 4.5L9.75 8l-3.5 3.5" />
                      </svg>
                    )}
                    {entry.keys && <Keycaps spec={entry.keys} />}
                  </button>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
