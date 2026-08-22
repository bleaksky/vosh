import { useEffect, useMemo, useRef, useState } from 'react';
import {
  buildAliasEntries,
  buildPaletteEntries,
  filterEntries,
  type PaletteDeps,
  type PaletteEntry,
  type PaletteGroup,
} from '../lib/palette';

interface Props {
  deps: PaletteDeps;
  onClose: () => void;
}

const GROUP_ORDER: PaletteGroup[] = ['commands', 'aliases', 'panes', 'settings'];
const SCOPES: ('all' | PaletteGroup)[] = ['all', 'commands', 'aliases', 'panes', 'settings'];

function GroupIcon({ group }: { group: PaletteGroup }) {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 15 15"
      fill="none"
      strokeWidth="1.3"
      aria-hidden="true"
    >
      {group === 'commands' && <path d="M3 3.5 7 7.5 3 11.5M8 11.5h4" />}
      {group === 'aliases' && <path d="M8.5 1.5 4 8.5h3l-1 5L11 6.5H8z" />}
      {group === 'panes' && (
        <>
          <rect x="2" y="3" width="11" height="9" rx="1" />
          <path d="M8.5 3v9" />
        </>
      )}
      {group === 'settings' && (
        <>
          <circle cx="7.5" cy="7.5" r="2.2" />
          <path d="M7.5 1.5v2M7.5 11.5v2M1.5 7.5h2M11.5 7.5h2M3.3 3.3l1.4 1.4M10.3 10.3l1.4 1.4M11.7 3.3l-1.4 1.4M4.7 10.3l-1.4 1.4" />
        </>
      )}
    </svg>
  );
}

// The ⌘K command palette from the Ember canvas: scrim, raised panel,
// scope chips, grouped results with type icons, keyboard-first.
// Entries rebuild on every open so labels track live state; aliases
// stream in async from the backend.
export function CommandPalette({ deps, onClose }: Props) {
  const [query, setQuery] = useState('');
  const [scope, setScope] = useState<'all' | PaletteGroup>('all');
  const [selected, setSelected] = useState(0);
  const [aliasEntries, setAliasEntries] = useState<PaletteEntry[]>([]);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const baseEntries = useMemo(() => buildPaletteEntries(deps), [deps]);

  useEffect(() => {
    inputRef.current?.focus();
    let cancelled = false;
    void buildAliasEntries(deps).then((rows) => {
      if (!cancelled) setAliasEntries(rows);
    });
    return () => {
      cancelled = true;
    };
    // deps is stable for the lifetime of one palette open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const visible = useMemo(() => {
    const all = [...baseEntries, ...aliasEntries];
    const scoped = scope === 'all' ? all : all.filter((e) => e.group === scope);
    const filtered = filterEntries(scoped, query);
    // Stable group presentation: keep result order inside each group,
    // groups in fixed order.
    return GROUP_ORDER.flatMap((g) => filtered.filter((e) => e.group === g));
  }, [baseEntries, aliasEntries, scope, query]);

  useEffect(() => {
    setSelected(0);
  }, [query, scope, aliasEntries.length]);

  const run = (entry: PaletteEntry | undefined) => {
    if (!entry) return;
    onClose();
    void entry.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((s) => Math.min(s + 1, visible.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((s) => Math.max(s - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      run(visible[selected]);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      const dir = e.shiftKey ? -1 : 1;
      setScope((s) => SCOPES[(SCOPES.indexOf(s) + dir + SCOPES.length) % SCOPES.length]);
    }
  };

  // Keep the selected row scrolled into view during keyboard nav.
  useEffect(() => {
    const row = listRef.current?.querySelector('[data-selected="true"]');
    row?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  let flatIndex = -1;
  return (
    <div className="palette-scrim" data-occludes-surface="true" onPointerDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-label="command palette"
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <div className="palette-input-row">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" strokeWidth="1.3">
            <circle cx="6" cy="6" r="4" />
            <path d="M9 9l3.5 3.5" />
          </svg>
          <input
            ref={inputRef}
            type="text"
            value={query}
            placeholder="type a command, alias, pane, or setting"
            spellCheck={false}
            aria-label="palette query"
            onChange={(e) => setQuery(e.target.value)}
          />
          <span className="kbd">esc</span>
        </div>
        <div className="palette-scopes">
          {SCOPES.map((s) => (
            <button
              key={s}
              type="button"
              className={`palette-scope${scope === s ? ' is-active' : ''}`}
              onClick={() => setScope(s)}
            >
              {s}
            </button>
          ))}
        </div>
        <div className="palette-results" ref={listRef}>
          {visible.length === 0 && <div className="palette-empty">nothing matches</div>}
          {GROUP_ORDER.map((g) => {
            const rows = visible.filter((e) => e.group === g);
            if (rows.length === 0) return null;
            return (
              <div key={g}>
                <div className="caps palette-group">{g}</div>
                {rows.map((entry) => {
                  flatIndex += 1;
                  const idx = flatIndex;
                  const isSel = idx === selected;
                  return (
                    <button
                      key={entry.id}
                      type="button"
                      className={`palette-row${isSel ? ' is-selected' : ''}`}
                      data-selected={isSel}
                      onPointerEnter={() => setSelected(idx)}
                      onClick={() => run(entry)}
                    >
                      <GroupIcon group={entry.group} />
                      <span className="palette-row-title">{entry.title}</span>
                      {entry.hint && <span className="palette-row-hint">{entry.hint}</span>}
                      {isSel && <span className="palette-row-run">run</span>}
                      {entry.kbd ? (
                        <span className="kbd">{entry.kbd}</span>
                      ) : (
                        isSel && <span className="kbd">&#9166;</span>
                      )}
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
        <div className="palette-footer">
          <span>&#8593;&#8595; navigate</span>
          <span>&#9166; run</span>
          <span>tab scope</span>
          <span className="palette-footer-count">
            {visible.length === 0
              ? '0'
              : `${Math.min(selected + 1, visible.length)} of ${visible.length}`}
          </span>
        </div>
      </div>
    </div>
  );
}
