import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { HELP_SECTIONS, HELP_TOPICS, searchTopics, type HelpTopic } from '../lib/helpContent';

interface Props {
  /** Close the modal. Hooked to backdrop click, the [close] button,
   *  and the Esc key. The parent owns visibility state. */
  onClose: () => void;
}

// In-client help modal. A frameless overlay with a left-side topic
// rail (grouped by section) and a body pane on the right. A search
// input at the top filters topics live and highlights matches inside
// the result body. Backdrop click and Esc dismiss the modal.
//
// The content is loaded statically from helpContent.ts so search and
// rendering stay synchronous and zero-dependency. The same data module
// drives the mirror HELP.md file at the repo root.
export function HelpView({ onClose }: Props) {
  const [query, setQuery] = useState('');
  // Selected topic id. Defaults to the first topic. Search results
  // also auto-select the first match so the body never goes blank
  // while a query has hits.
  const [selectedId, setSelectedId] = useState<string>(HELP_TOPICS[0]?.id ?? '');
  const searchRef = useRef<HTMLInputElement | null>(null);

  // Filter topics live. Empty query returns every topic.
  const filtered = useMemo(() => searchTopics(query, HELP_TOPICS), [query]);

  // When the search query changes, keep selection valid. If the
  // currently selected topic is filtered out, jump to the first
  // matching topic so the body never shows an empty selection on
  // top of search hits. When the user clears the query, selection
  // sticks where it already was.
  useEffect(() => {
    if (filtered.length === 0) return;
    if (filtered.some((t) => t.id === selectedId)) return;
    setSelectedId(filtered[0].id);
  }, [filtered, selectedId]);

  // Auto-focus the search input on mount so the user can start
  // typing immediately. The setTimeout dodges a Tauri quirk where
  // focus calls during the same tick as window.show() get dropped.
  useEffect(() => {
    const t = window.setTimeout(() => searchRef.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, []);

  // Global Esc closes the modal. Attached at window level so it
  // works regardless of which element holds focus.
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const selected = filtered.find((t) => t.id === selectedId) ?? filtered[0] ?? null;
  const highlight = query.trim().length > 0 ? query.trim() : null;

  // Group filtered topics by section, preserving the canonical
  // section order from HELP_SECTIONS. Sections that have no topics
  // in the filtered list collapse entirely so the rail does not
  // render empty section headings during search.
  const grouped: Array<{ section: string; topics: HelpTopic[] }> = HELP_SECTIONS.map((section) => ({
    section,
    topics: filtered.filter((t) => t.section === section),
  })).filter((g) => g.topics.length > 0);

  return (
    <div className="help-modal-backdrop" onClick={onClose}>
      <div
        className="help-modal"
        role="dialog"
        aria-label="Vosh help"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="help-modal-header">
          <span className="help-modal-title">help</span>
          <input
            ref={searchRef}
            type="search"
            className="help-modal-search"
            placeholder="search help"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => handleSearchKeyDown(e, filtered, selectedId, setSelectedId)}
            aria-label="search help topics"
          />
          <button
            type="button"
            className="help-modal-close"
            onClick={onClose}
            aria-label="close help"
          >
            close
          </button>
        </header>

        <div className="help-modal-shell">
          <nav className="help-modal-rail" aria-label="help topics">
            {grouped.length === 0 ? (
              <div className="help-modal-empty">no help topics match that</div>
            ) : (
              grouped.map((g) => (
                <div key={g.section} className="help-modal-section">
                  <div className="help-modal-section-title">{g.section}</div>
                  {g.topics.map((t) => (
                    <button
                      key={t.id}
                      type="button"
                      className={`help-modal-topic${t.id === selected?.id ? ' is-active' : ''}`}
                      aria-pressed={t.id === selected?.id}
                      onClick={() => setSelectedId(t.id)}
                    >
                      <span className="help-modal-topic-num">{t.number}</span>
                      <span className="help-modal-topic-title">
                        {renderInline(t.title, highlight)}
                      </span>
                    </button>
                  ))}
                </div>
              ))
            )}
          </nav>

          <div className="help-modal-body">
            {selected ? (
              <article aria-labelledby="help-modal-heading">
                <div className="help-modal-eyebrow">{selected.section}</div>
                <h2 id="help-modal-heading" className="help-modal-heading">
                  <span className="help-modal-heading-num">{selected.number}</span>
                  {renderInline(selected.title, highlight)}
                </h2>
                {renderBody(selected.body, highlight)}
              </article>
            ) : (
              <div className="help-modal-empty">
                no help topics match that. clear the search to see everything.
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// Down-arrow on the search input moves selection to the next visible
// topic; Up-arrow walks backward. Enter is a no-op (the input is a
// search field, not a submit-form trigger). Tab still does normal
// focus traversal so screen-readers and keyboard-only users land on
// the rail buttons after the search input.
function handleSearchKeyDown(
  e: KeyboardEvent<HTMLInputElement>,
  filtered: HelpTopic[],
  selectedId: string,
  setSelectedId: (id: string) => void,
): void {
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
  if (filtered.length === 0) return;
  e.preventDefault();
  const currentIdx = Math.max(
    0,
    filtered.findIndex((t) => t.id === selectedId),
  );
  const step = e.key === 'ArrowDown' ? 1 : -1;
  const nextIdx = (currentIdx + step + filtered.length) % filtered.length;
  setSelectedId(filtered[nextIdx].id);
}

/**
 * Render a help-body string into React nodes. The body uses a tiny
 * markdown subset:
 *   - Paragraphs separate with a blank line.
 *   - A block whose lines all start with "- " renders as a bulleted list.
 *   - Inline backticks wrap code in a `<code>` span.
 * When `highlight` is set, every case-insensitive substring match wraps
 * in a `<mark>` element so search hits visibly stand out.
 */
function renderBody(body: string, highlight: string | null): ReactNode {
  const blocks = body
    .split(/\n\n+/)
    .map((b) => b.trim())
    .filter((b) => b.length > 0);
  return blocks.map((block, i) => {
    const lines = block.split('\n');
    const isList = lines.length > 0 && lines.every((l) => l.startsWith('- '));
    if (isList) {
      return (
        <ul key={i} className="help-modal-list">
          {lines.map((line, j) => (
            <li key={j} className="help-modal-list-item">
              {renderInline(line.slice(2), highlight)}
            </li>
          ))}
        </ul>
      );
    }
    return (
      <p key={i} className="help-modal-p">
        {renderInline(block, highlight)}
      </p>
    );
  });
}

/**
 * Render inline text. First splits on single backticks to lift inline
 * code into `<code>` spans, then walks each resulting segment to wrap
 * search hits in `<mark>`. Unmatched backticks (odd count) render the
 * trailing backtick as a literal character so the user sees their
 * typo rather than losing trailing text.
 */
function renderInline(text: string, highlight: string | null): ReactNode {
  const segments: Array<{ kind: 'text' | 'code'; text: string }> = [];
  let i = 0;
  while (i < text.length) {
    const open = text.indexOf('`', i);
    if (open < 0) {
      segments.push({ kind: 'text', text: text.slice(i) });
      break;
    }
    if (open > i) segments.push({ kind: 'text', text: text.slice(i, open) });
    const close = text.indexOf('`', open + 1);
    if (close < 0) {
      segments.push({ kind: 'text', text: text.slice(open) });
      break;
    }
    segments.push({ kind: 'code', text: text.slice(open + 1, close) });
    i = close + 1;
  }
  return segments.map((seg, k) => {
    if (seg.kind === 'code') {
      return (
        <code key={k} className="help-modal-code">
          {highlightText(seg.text, highlight)}
        </code>
      );
    }
    return <span key={k}>{highlightText(seg.text, highlight)}</span>;
  });
}

/**
 * Wrap every case-insensitive match of `highlight` inside `text` in a
 * `<mark>` element. Returns the original string when there is no
 * highlight or no match, so React can short-circuit a string render
 * instead of mounting a span tree.
 */
function highlightText(text: string, highlight: string | null): ReactNode {
  if (!highlight) return text;
  const lower = text.toLowerCase();
  const q = highlight.toLowerCase();
  if (q.length === 0 || !lower.includes(q)) return text;
  const parts: ReactNode[] = [];
  let pos = 0;
  while (pos < text.length) {
    const idx = lower.indexOf(q, pos);
    if (idx < 0) {
      parts.push(text.slice(pos));
      break;
    }
    if (idx > pos) parts.push(text.slice(pos, idx));
    parts.push(
      <mark key={`m${idx}`} className="help-modal-hit">
        {text.slice(idx, idx + q.length)}
      </mark>,
    );
    pos = idx + q.length;
  }
  return parts;
}
