import {
  useEffect,
  useId,
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
  type MutableRefObject,
} from 'react';
import { HELP_SECTIONS, type HelpTopic } from './helpContent';
import { helpSearchKey, sectionTopics } from './helpNav';
import { shortcutKeys } from '../lib/shortcuts';
import { scrollWithin } from '../lib/scrollWithin';
import { Keycap, SearchIcon, VisuallyHidden } from '../ui';
import { sectionIcon } from './sectionIcons';

// The Help sidebar (the approved Help boards), the Settings sidebar 280
// wide: the drag strip under the traffic lights, the search pill at
// (12, 44) with its keycaps, and the nine sections as Settings rows
// from y 84. The section you are in opens under itself with its topics
// at the label's x, and the topic you read carries the selected row
// fill. While the search holds words its results replace the nav, best
// first, and the article shows the one you are on.

interface Props {
  /** The topic the article shows. */
  topic: HelpTopic;
  /** The section open in the nav, or null with every one closed. */
  openSection: string | null;
  onToggleSection: (section: string) => void;
  onOpenTopic: (topic: HelpTopic) => void;
  query: string;
  onQuery: (query: string) => void;
  /** The search results, best first. */
  results: HelpTopic[];
  /** The result the article shows. */
  active: number;
  onActive: (index: number) => void;
  /** Enter in the search: the next match, or the previous with Shift. */
  onStep: (step: 1 | -1) => void;
  inputRef: MutableRefObject<HTMLInputElement | null>;
  mac: boolean;
}

export function HelpSidebar({
  topic,
  openSection,
  onToggleSection,
  onOpenTopic,
  query,
  onQuery,
  results,
  active,
  onActive,
  onStep,
  inputRef,
  mac,
}: Props) {
  const inputId = useId();
  const listId = useId();
  const searching = query.trim().length > 0;
  const optionId = (index: number) => `${listId}-option-${index}`;
  const navRef = useRef<HTMLElement | null>(null);

  // Keep the topic you read in view in the nav.
  useEffect(() => {
    if (searching) return;
    scrollWithin(navRef.current?.querySelector('[aria-current="page"]'), { block: 'nearest' });
  }, [topic.id, openSection, searching]);

  useEffect(() => {
    if (!searching) return;
    scrollWithin(document.getElementById(optionId(active)), { block: 'nearest' });
    // optionId only reads listId, which never changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, searching]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    const action = helpSearchKey(e.key, e.shiftKey, {
      active,
      results: results.length,
      query,
    });
    if (!action) return;
    e.preventDefault();
    if (action.kind === 'active') onActive(action.index);
    else if (action.kind === 'step') onStep(action.by);
    else if (action.kind === 'clear') onQuery('');
    else e.currentTarget.blur();
  };

  return (
    <aside className="st-sidebar">
      <div className="st-drag" data-tauri-drag-region="" />
      <div className="st-search">
        <span className="st-search-icon" aria-hidden="true">
          <SearchIcon />
        </span>
        <label htmlFor={inputId} className="visually-hidden">
          Search help
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
          aria-activedescendant={searching && results.length > 0 ? optionId(active) : undefined}
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          onKeyDown={onKeyDown}
        />
        {!searching && (
          <span className="keys st-search-keys" aria-hidden="true">
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
              {results.map((t, index) => (
                <div
                  key={t.id}
                  id={optionId(index)}
                  role="option"
                  aria-selected={index === active}
                  className="st-nav-item"
                  // Keep the caret in the search while you click.
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => onActive(index)}
                >
                  {sectionIcon(t.section)}
                  <span className="st-nav-label">{t.title}</span>
                  <VisuallyHidden>, in {t.section}</VisuallyHidden>
                </div>
              ))}
            </div>
          ) : (
            <p id={listId} className="st-results-empty" role="status">
              No help matches.
            </p>
          )}
        </div>
      ) : (
        <nav ref={navRef} aria-label="Help topics" className="st-nav hp-nav">
          <ul>
            {HELP_SECTIONS.map((section) => {
              const open = section === openSection;
              const sectionId = `hp-section-${HELP_SECTIONS.indexOf(section)}`;
              return (
                <li key={section}>
                  <button
                    type="button"
                    className="st-nav-item hp-section"
                    aria-expanded={open}
                    aria-controls={open ? sectionId : undefined}
                    onClick={() => onToggleSection(section)}
                  >
                    {sectionIcon(section)}
                    <span className="st-nav-label">{section}</span>
                  </button>
                  {open && (
                    <ul id={sectionId}>
                      {sectionTopics(section).map((t) => (
                        <li key={t.id}>
                          <a
                            href={`#${t.id}`}
                            className="st-nav-item hp-topic"
                            aria-current={t.id === topic.id ? 'page' : undefined}
                            onClick={(e) => {
                              e.preventDefault();
                              onOpenTopic(t);
                            }}
                          >
                            <span className="st-nav-label">{t.title}</span>
                          </a>
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        </nav>
      )}
    </aside>
  );
}
