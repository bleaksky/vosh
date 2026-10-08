import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from 'react';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { subscribeSettingsFind } from '../ipc/windows';
import { ariaKeyshortcuts, shortcutKey, shortcutKeys } from '../lib/shortcuts';
import { scrollWithin } from '../lib/scrollWithin';
import {
  SETTINGS_GROUPS,
  settingsGroupLabel,
  type SettingsGroup,
  type SettingsTarget,
} from '../lib/settingsNav';
import { searchSettingsRows, settingsRowKey, type SettingsRowEntry } from './settingsSearch';
import {
  AccessibilityIcon,
  AppearanceIcon,
  BoltIcon,
  CodeIcon,
  GearIcon,
  Keycap,
  KeyboardIcon,
  LayoutIcon,
  LogsIcon,
  SearchIcon,
  TerminalIcon,
  UserIcon,
  VisuallyHidden,
  VitalsIcon,
} from '../ui';

// The Settings sidebar (the approved boards): a 32 px drag strip where
// macOS puts the traffic lights, the search pill at (12, 44), and the
// nav of eleven groups at (8, 84), in four clusters set off by 13 px
// gaps with no headings (Settings layout Q9). Prompt wears the terminal
// glyph Help draws for Play. While the search holds text its results
// replace the nav. Each result is a row, not a group. Enter or a click
// opens it, and the frame scrolls the row into view and flashes it.

const GROUP_ICONS: Record<SettingsGroup, () => ReactNode> = {
  general: () => <GearIcon />,
  appearance: () => <AppearanceIcon />,
  accessibility: () => <AccessibilityIcon />,
  layout: () => <LayoutIcon />,
  vitals: () => <VitalsIcon />,
  prompt: () => <TerminalIcon />,
  input: () => <KeyboardIcon />,
  automation: () => <BoltIcon />,
  scripts: () => <CodeIcon />,
  logs: () => <LogsIcon />,
  characters: () => <UserIcon />,
};

interface Props {
  group: SettingsGroup;
  onNavigate: (target: SettingsTarget) => void;
  pathB: boolean;
  mac: boolean;
}

export function Sidebar({ group, onNavigate, pathB, mac }: Props) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const inputId = useId();
  const listId = useId();
  const results = useMemo(() => searchSettingsRows(query, { pathB, mac }), [query, pathB, mac]);
  const searching = query.trim().length > 0;
  const optionId = (index: number) => `${listId}-option-${index}`;

  // Cmd+F on macOS, Ctrl+F elsewhere, focuses the search. Ctrl+F on
  // macOS stays the text field's own shortcut.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey || e.shiftKey) return;
      const mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (!mod || shortcutKey(e) !== 'f') return;
      e.preventDefault();
      inputRef.current?.focus();
      inputRef.current?.select();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [mac]);

  // Find in the macOS menu bar, chosen while Settings is in front,
  // searches settings the same way.
  useTauriEvent(subscribeSettingsFind, () => {
    inputRef.current?.focus();
    inputRef.current?.select();
  });

  useEffect(() => setActive(0), [query]);

  useEffect(() => {
    if (!searching) return;
    scrollWithin(document.getElementById(optionId(active)), { block: 'nearest' });
    // optionId only reads listId, which never changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, searching]);

  const pick = (row: SettingsRowEntry) => {
    setQuery('');
    onNavigate(row.target);
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown' && results.length > 0) {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === 'ArrowUp' && results.length > 0) {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      const row = results[active];
      if (row) {
        e.preventDefault();
        pick(row);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (query) setQuery('');
      else e.currentTarget.blur();
    }
  };

  return (
    <aside className="st-sidebar">
      <div className="st-drag" data-tauri-drag-region="" />
      <div className="st-search">
        <span className="st-search-icon" aria-hidden="true">
          <SearchIcon />
        </span>
        <label htmlFor={inputId} className="st-visually-hidden">
          Search settings
        </label>
        <input
          ref={inputRef}
          id={inputId}
          className="st-search-input"
          type="search"
          placeholder="Search"
          spellCheck={false}
          autoComplete="off"
          role="combobox"
          aria-expanded={searching}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-keyshortcuts={ariaKeyshortcuts('Mod+F', mac)}
          aria-activedescendant={searching && results.length > 0 ? optionId(active) : undefined}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
        />
        {!searching && (
          <span className="st-search-keys" aria-hidden="true">
            {shortcutKeys('Mod+F', mac).map((key) => (
              <Keycap key={key}>{key}</Keycap>
            ))}
          </span>
        )}
      </div>
      {searching ? (
        <div className="st-nav">
          {results.length > 0 ? (
            <div role="listbox" id={listId} aria-label="Search results">
              {results.map((row, index) => (
                <div
                  key={settingsRowKey(row)}
                  id={optionId(index)}
                  role="option"
                  aria-selected={index === active}
                  className="st-nav-item"
                  // Keep the caret in the search while you click.
                  onMouseDown={(e) => e.preventDefault()}
                  onMouseMove={() => setActive(index)}
                  onClick={() => pick(row)}
                >
                  {GROUP_ICONS[row.target.group]()}
                  <span className="st-nav-label">{row.label}</span>
                  <VisuallyHidden>, in {settingsGroupLabel(row.target.group)}</VisuallyHidden>
                </div>
              ))}
            </div>
          ) : (
            <p id={listId} className="st-results-empty" role="status">
              No settings match.
            </p>
          )}
        </div>
      ) : (
        <nav aria-label="Settings sections" className="st-nav">
          {SETTINGS_GROUPS.map((g) => (
            <a
              key={g.id}
              href={`#${g.id}`}
              className="st-nav-item"
              data-cluster={g.gap ? '' : undefined}
              aria-current={g.id === group ? 'page' : undefined}
              onClick={(e) => {
                e.preventDefault();
                onNavigate({ group: g.id });
              }}
            >
              {GROUP_ICONS[g.id]()}
              <span className="st-nav-label">{g.label}</span>
            </a>
          ))}
        </nav>
      )}
    </aside>
  );
}
