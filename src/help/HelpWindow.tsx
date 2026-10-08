import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getUiConfig, subscribeColorVisionChanged, type UiConfig } from '../ipc/uiConfig';
import { followReplacedUiConfig } from '../ipc/uiConfigBroadcast';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { subscribeHelpFind, subscribeHelpGoto } from '../ipc/windows';
import {
  applyThemePrefs,
  getCurrentThemeId,
  setColorVision,
  subscribeThemeChanges,
  subscribeThemePrefs,
} from '../theme/theme';
import { showAfterThemePaint } from '../lib/reveal';
import { customToAppTheme, findTheme, setCustomThemes } from '../theme/themes';
import { loadFontStack, renderFontStack } from '../lib/fontLoader';
import { findMarks } from '../theme/findMarks';
import { isMacPlatform, shortcutKey } from '../lib/shortcuts';
import { HELP_TOPICS, type HelpTopic } from './helpContent';
import {
  countMatches,
  helpScrollKey,
  landingOf,
  outlineFor,
  rankTopics,
  resolveHelpTarget,
  type HelpFocus,
  type HelpScroll,
  type HelpTarget,
} from './helpNav';
import { HELP_PENDING_KEY } from '../lib/helpLink';
import { scrollWithin } from '../lib/scrollWithin';
import { HelpSidebar } from './HelpSidebar';
import { HelpArticle } from './HelpArticle';
import { HelpOutline } from './HelpOutline';
import { WindowControls } from '../ui/WindowControls';
import { ChevronDownIcon, ChevronRightIcon, ChevronUpIcon, IconButton } from '../ui';

// The Help window, its own window like Settings and in the same frame:
// a 280 px sidebar with search and the nine sections, and a content
// column with the breadcrumb in the 32 px band over the article. On
// macOS the native traffic lights sit over the sidebar. Windows and
// Linux draw their controls at the right of the band.
//
// Every way into Help names a target (src/lib/helpLink.ts): a topic from
// a Settings book button or the palette, or words from `#help <words>`.

/** The topic you read last, so Help opens on it again. */
const LAST_TOPIC_KEY = 'vosh.help.topic';

function topicById(id: string | null): HelpTopic | undefined {
  return id === null ? undefined : HELP_TOPICS.find((t) => t.id === id);
}

function readStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** The target a cold open should land on, left by another window. */
function takePendingTarget(): HelpTarget | null {
  const pending = readStorage(HELP_PENDING_KEY);
  clearPendingTarget();
  return pending ? resolveHelpTarget(pending) : null;
}

function clearPendingTarget() {
  try {
    localStorage.removeItem(HELP_PENDING_KEY);
  } catch {
    // Storage unavailable. Nothing was left there.
  }
}

/** Where focus sits, for the keys that scroll the article. */
function focusIn(scroller: HTMLElement): HelpFocus {
  const el = document.activeElement;
  if (!el || el === document.body || el === document.documentElement) return 'none';
  if (scroller.contains(el)) return 'article';
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return 'field';
  return 'control';
}

/** A line of the article, about two lines of body text. */
const LINE_STEP = 40;

/** Scroll the article the way the key would if it had focus. A page
 *  keeps one line of the last page in view. */
function scrollArticle(scroller: HTMLElement, move: HelpScroll) {
  if (move.kind === 'edge') {
    scroller.scrollTop = move.to === 'top' ? 0 : scroller.scrollHeight;
  } else {
    const step =
      move.kind === 'page' ? Math.max(scroller.clientHeight - LINE_STEP, LINE_STEP) : LINE_STEP;
    scroller.scrollBy({ top: step * move.by });
  }
}

/** The mark fills: the theme's ANSI yellow at 28% for every match and
 *  60% for the one you are on, the way the terminal find marks them. */
function markColors(themeId: string): { fill: string; current: string } | null {
  const marks = findMarks(findTheme(themeId).xterm);
  return marks ? { fill: marks.match, current: marks.current } : null;
}

export function HelpWindow() {
  const mac = isMacPlatform();
  const [landing] = useState(() => takePendingTarget());
  const [topicId, setTopicId] = useState<string>(
    () =>
      (landing?.kind === 'topic' ? landing.topic.id : null) ??
      topicById(readStorage(LAST_TOPIC_KEY))?.id ??
      HELP_TOPICS[0].id,
  );
  const [query, setQuery] = useState(landing?.kind === 'search' ? landing.query : '');
  const [active, setActive] = useState(0);
  const [match, setMatch] = useState(0);
  const [openSection, setOpenSection] = useState<string | null>(null);
  const [titleGone, setTitleGone] = useState(false);
  /** Counts the links that landed, so each one starts its page at the
   *  top. */
  const [landed, setLanded] = useState(0);
  const [themeId, setThemeId] = useState(() => getCurrentThemeId());
  const [config, setConfig] = useState<UiConfig | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const titleRef = useRef<HTMLHeadingElement | null>(null);

  const searching = query.trim().length > 0;
  const results = useMemo(() => rankTopics(query), [query]);
  // While the search holds words the article shows the result you are
  // on, and keeps it once you clear the search.
  const shown = (searching ? results[active] : undefined) ?? topicById(topicId) ?? HELP_TOPICS[0];
  const matches = searching ? countMatches(shown, query) : 0;
  const outline = useMemo(() => outlineFor(shown), [shown]);
  const marks = useMemo(() => markColors(themeId), [themeId]);

  const changeQuery = useCallback(
    (next: string) => {
      if (next.trim().length === 0) setTopicId(shown.id);
      setQuery(next);
      setActive(0);
      setMatch(0);
    },
    [shown.id],
  );

  // A link opens its section in the sidebar and starts its page at the
  // top, even when it lands on the topic you are reading.
  const land = useCallback((target: HelpTarget) => {
    const next = landingOf(target);
    if (next.topicId !== null) setTopicId(next.topicId);
    setQuery(next.query);
    setActive(0);
    setMatch(0);
    if (next.section !== null) setOpenSection(next.section);
    setLanded((n) => n + 1);
    if (next.focusSearch) inputRef.current?.focus();
  }, []);

  // Help that opens on words, from `#help <words>`, puts the caret in
  // the search that holds them, so Enter steps through the matches.
  useEffect(() => {
    if (landing?.kind === 'search') inputRef.current?.focus();
  }, [landing]);

  // The section of the topic you read opens in the nav.
  useEffect(() => {
    setOpenSection(shown.section);
    try {
      localStorage.setItem(LAST_TOPIC_KEY, shown.id);
    } catch {
      // Storage unavailable. Help opens on the first topic next time.
    }
  }, [shown.id, shown.section]);

  // A new topic, or a link that lands, starts at the top. While you
  // search, the match you are on scrolls into view instead.
  useLayoutEffect(() => {
    const root = scrollRef.current;
    if (!root) return;
    const current = matches > 0 ? root.querySelector('[data-current]') : null;
    if (current) scrollWithin(current, { block: 'center' });
    else root.scrollTop = 0;
  }, [shown.id, query, match, matches, landed]);

  // Once the title scrolls under the band, the breadcrumb names it.
  useEffect(() => {
    const root = scrollRef.current;
    const title = titleRef.current;
    if (!root || !title || typeof IntersectionObserver === 'undefined') return;
    setTitleGone(false);
    const observer = new IntersectionObserver(
      ([entry]) =>
        setTitleGone(
          !entry.isIntersecting && entry.boundingClientRect.top < (entry.rootBounds?.top ?? 0),
        ),
      { root },
    );
    observer.observe(title);
    return () => observer.disconnect();
  }, [shown.id]);

  // A link for a window that is already open. The window that asked
  // also left the target in storage for a cold open, so clear it.
  useTauriEvent(subscribeHelpGoto, (link) => {
    if (typeof link !== 'string') return;
    clearPendingTarget();
    const target = resolveHelpTarget(link);
    if (target) land(target);
    void getCurrentWindow().setFocus();
  });

  // Cmd+F on macOS, Ctrl+F elsewhere, and Find in the menu bar, focus
  // the search.
  const focusSearch = () => {
    inputRef.current?.focus();
    inputRef.current?.select();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey || e.shiftKey) return;
      const mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (!mod || shortcutKey(e) !== 'f') return;
      e.preventDefault();
      focusSearch();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [mac]);
  useTauriEvent(subscribeHelpFind, focusSearch);

  // The article takes focus from Tab and scrolls itself then. From the
  // search or the sidebar, the keys that scroll a page still reach it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const scroller = scrollRef.current;
      if (!scroller || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      const move = helpScrollKey(e.key, e.shiftKey, focusIn(scroller));
      if (!move) return;
      e.preventDefault();
      scrollArticle(scroller, move);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  // Load the theme and the font, and show the window once a frame with
  // the theme has gone out. The startup paint usually has it on screen
  // already.
  const take = (cfg: UiConfig) => {
    setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
    applyThemePrefs(cfg);
    setThemeId(getCurrentThemeId());
    setConfig(cfg);
  };
  useEffect(() => {
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      void win.show().then(() => win.setFocus());
    };
    const fallback = window.setTimeout(reveal, 500);
    getUiConfig()
      .then(take)
      .catch((e: unknown) => console.error('[help] reading the config failed', e))
      .finally(() => showAfterThemePaint(reveal));
    return () => window.clearTimeout(fallback);
  }, []);
  // A profile switch or an import replaces the config: the theme and
  // the font may change with it.
  useTauriEvent(
    (cb) =>
      followReplacedUiConfig(cb, (e) => console.error('[help] following the config failed', e)),
    take,
  );
  // Another window changed the theme. The repaint has run already. A
  // custom theme Help has not read yet comes with the config.
  useTauriEvent(subscribeThemeChanges, () => {
    setThemeId(getCurrentThemeId());
    getUiConfig()
      .then(take)
      .catch(() => {});
  });
  useTauriEvent(subscribeThemePrefs, (prefs) => {
    applyThemePrefs(prefs);
    setThemeId(getCurrentThemeId());
  });
  // Settings saved another color vision, which the status colors here
  // follow too.
  useTauriEvent(subscribeColorVisionChanged, setColorVision);

  // Commands and codes use your terminal font through --font-mud, the
  // way Settings and the main window do.
  const fontFamily = config?.font_family;
  useEffect(() => {
    if (!fontFamily) return;
    const rendered = renderFontStack(fontFamily);
    loadFontStack(rendered);
    document.documentElement.style.setProperty('--app-font-family', rendered);
  }, [fontFamily]);

  const step = (by: 1 | -1) => {
    if (matches === 0) return;
    setMatch((m) => (m + by + matches) % matches);
  };

  return (
    <div className="st-app hp-app window-edge">
      <HelpSidebar
        topic={shown}
        openSection={openSection}
        onToggleSection={(section) => setOpenSection((open) => (open === section ? null : section))}
        onOpenTopic={(t) => setTopicId(t.id)}
        query={query}
        onQuery={changeQuery}
        results={results}
        active={active}
        onActive={(index) => {
          setActive(index);
          setMatch(0);
        }}
        onStep={step}
        inputRef={inputRef}
        mac={mac}
      />
      <main className="st-main">
        <header className="st-header" data-tauri-drag-region="">
          <div className="st-crumb" data-tauri-drag-region="">
            <span className="st-crumb-root" data-tauri-drag-region="">
              Help
            </span>
            <ChevronRightIcon size={12} className="st-crumb-sep" />
            {titleGone ? (
              <>
                <span className="st-crumb-root" data-tauri-drag-region="">
                  {shown.section}
                </span>
                <ChevronRightIcon size={12} className="st-crumb-sep" />
                <span className="st-crumb-title" data-tauri-drag-region="">
                  {shown.title}
                </span>
              </>
            ) : (
              <span className="st-crumb-title" data-tauri-drag-region="">
                {shown.section}
              </span>
            )}
          </div>
          {matches > 0 && (
            <div className="hp-matches">
              <span className="hp-match-count" role="status">
                {match + 1} of {matches}
              </span>
              <IconButton
                label="Previous match"
                icon={<ChevronUpIcon />}
                onClick={() => step(-1)}
              />
              <IconButton label="Next match" icon={<ChevronDownIcon />} onClick={() => step(1)} />
            </div>
          )}
          {!mac && <WindowControls />}
        </header>
        <div
          ref={scrollRef}
          className="hp-scroll"
          tabIndex={0}
          role="region"
          aria-label={shown.title}
        >
          <div className="hp-page" data-outline={outline ? '' : undefined}>
            <HelpArticle
              ref={titleRef}
              topic={shown}
              query={searching ? query : ''}
              current={match}
              outline={outline}
              markColors={marks}
            />
            {outline && <HelpOutline key={shown.id} entries={outline} scrollRef={scrollRef} />}
          </div>
        </div>
      </main>
    </div>
  );
}
