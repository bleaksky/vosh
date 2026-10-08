import { useCallback, useEffect, useRef, useState, type ComponentType } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  AFFECTS_DISPLAY_FIELDS,
  affectsDisplayFields,
  affectsDisplayOf,
  sameAffectsDisplay,
  subscribeAffectsDisplayChanged,
} from '../ipc/affects';
import { loadoutsGetState, subscribeLoadoutsChanged } from '../ipc/loadouts';
import { subscribeProfilesChanged } from '../ipc/profiles';
import { THEME_PREFS_FIELDS } from '../ipc/theme';
import {
  fetchUiConfig,
  shownStyle,
  subscribeUiConfigReplaced,
  subscribeVitalsOptionsChanged,
  subscribeVitalsTextChanged,
  type UiConfig,
  type UiFields,
} from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { subscribeSettingsGotoTab } from '../ipc/windows';
import {
  applyThemePrefs,
  setColorVision,
  subscribeThemeChanges,
  subscribeThemePrefs,
  themeFollowOf,
} from '../theme/theme';
import { showAfterThemePaint } from '../lib/reveal';
import { customToAppTheme, setCustomThemes } from '../theme/themes';
import { loadFontStack, renderFontStack } from '../lib/fontLoader';
import { isMacPlatform } from '../lib/shortcuts';
import {
  leavesSettingsPage,
  resolveSettingsTarget,
  settingsGroupLabel,
  settingsScrollIds,
  settingsSubpage,
  type SettingsGroup,
  type SettingsTarget,
} from '../lib/settingsNav';
import { SETTINGS_PENDING_KEY } from '../lib/settingsLink';
import { revealSettingsAnchor } from './revealAnchor';
import { getShownProfile, isShownHeld, subscribeShownMoves } from './shownProfile';
import { ShownSession } from './ShownSession';
import { Sidebar } from './Sidebar';
import { useSettingsClose } from './useSettingsClose';
import { settingsSaveHolds } from './useSettingsAutoSave';
import { vitalsStylePick } from '../panel/vitalsView';
import { WindowControls } from '../ui/WindowControls';
import { ChevronRightIcon } from '../ui';
import type { LeaveGuard, SettingsPageProps } from './pageTypes';
import { GeneralPage } from './general/GeneralPage';
import { LayoutPage } from './layout/LayoutPage';
import { InputPage } from './input/InputPage';
import { AutomationPage } from './automation/AutomationPage';
import { CharactersPage } from './characters/CharactersPage';
import { AppearancePage } from './appearance/AppearancePage';
import { ScriptsPage } from './scripts/ScriptsPage';

// The Settings window (the approved Settings boards). A 200 px sidebar
// with search and the seven group nav, and a content column with the
// breadcrumb in the 32 px band over the group's page. With two or more
// sessions open, the band names the session and the profile Settings
// edits at its right (ShownSession.tsx). On macOS the
// native traffic lights sit over the sidebar. Windows and Linux draw
// minimize, maximize, and close at the right of the band.
//
// Every way into Settings names a target (src/lib/settingsNav.ts): the
// nav, a search hit, a deep link from the main window. The frame shows
// the target's group, hands the page the target, and scrolls to the
// anchor the target names.

interface GroupPage {
  Page: ComponentType<SettingsPageProps>;
  /** The page pins its own bar and scrolls inside itself, so the
   *  content column does not scroll. A function decides per target,
   *  for a group whose page inside it scrolls on its own. */
  selfScroll?: boolean | ((target: SettingsTarget) => boolean);
}

const PAGES: Record<SettingsGroup, GroupPage> = {
  // The session logs page pins its toolbar over the results.
  general: { Page: GeneralPage, selfScroll: (target) => settingsSubpage(target) !== null },
  appearance: { Page: AppearancePage },
  layout: { Page: LayoutPage },
  input: { Page: InputPage },
  automation: { Page: AutomationPage, selfScroll: true },
  // A plugin's page pins its editor and Output to the window.
  scripts: { Page: ScriptsPage, selfScroll: (target) => settingsSubpage(target) !== null },
  characters: { Page: CharactersPage },
};

/** The target a cold open should land on, left by the main window. */
function takePendingTarget(): SettingsTarget | null {
  try {
    const pending = localStorage.getItem(SETTINGS_PENDING_KEY);
    localStorage.removeItem(SETTINGS_PENDING_KEY);
    return pending ? resolveSettingsTarget(pending) : null;
  } catch {
    return null;
  }
}

function clearPendingTarget() {
  try {
    localStorage.removeItem(SETTINGS_PENDING_KEY);
  } catch {
    // Storage unavailable. Nothing was left there.
  }
}

/** The fields the vitals menu picks. */
const VITALS_MENU_FIELDS: readonly (keyof UiFields)[] = [
  'vitals_style',
  'vitals_density',
  'vitals_values',
];

/** The fields the vitals text card saves. */
const VITALS_TEXT_FIELDS: readonly (keyof UiFields)[] = ['vitals_text', 'vitals_text_previous'];

export function SettingsWindow() {
  const mac = isMacPlatform();
  const [nav, setNav] = useState<{ target: SettingsTarget; seq: number }>(() => ({
    target: takePendingTarget() ?? { group: 'general' },
    seq: 0,
  }));
  const [config, setConfig] = useState<UiConfig | null>(null);
  // The latest config for event handlers, which outlive a render.
  const configRef = useRef<UiConfig | null>(null);
  useEffect(() => {
    configRef.current = config;
  }, [config]);
  // This window paints its status colors for your color vision, which a
  // pick on Appearance changes here first.
  const colorVision = config?.color_vision;
  useEffect(() => {
    if (colorVision !== undefined) setColorVision(colorVision);
  }, [colorVision]);
  const [error, setError] = useState<string | null>(null);
  const [pathB, setPathB] = useState(false);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const targetRef = useRef(nav.target);
  const leaveGuardRef = useRef<LeaveGuard | null>(null);
  const setLeaveGuard = useCallback((guard: LeaveGuard | null) => {
    leaveGuardRef.current = guard;
  }, []);

  useEffect(() => {
    targetRef.current = nav.target;
  }, [nav]);

  // Closing the window, and quitting, send every write still waiting,
  // the field you are typing in included.
  useSettingsClose();

  // A page with unsaved changes can hold a move to another group, or to
  // another page inside its group, until you answer its question. Any
  // other move inside the page is the page's own.
  const go = useCallback((target: SettingsTarget) => {
    const move = () => {
      setError(null);
      setNav((prev) => ({ target, seq: prev.seq + 1 }));
    };
    const guard = leaveGuardRef.current;
    if (guard && leavesSettingsPage(targetRef.current, target) && guard(move)) return;
    move();
  }, []);

  // A deep link for a window that is already open. The main window
  // also left the target in storage for a cold open, so clear it, or
  // the next cold open would land here again.
  useTauriEvent(subscribeSettingsGotoTab, (target) => {
    if (typeof target !== 'string') return;
    clearPendingTarget();
    go(resolveSettingsTarget(target));
    void getCurrentWindow().setFocus();
  });

  // Scroll to the anchor the target names once the page draws it, or
  // to the top for a bare group.
  useEffect(() => {
    const root = contentRef.current;
    if (!root) return;
    const ids = settingsScrollIds(nav.target);
    if (ids.length === 0) {
      root.scrollTop = 0;
      return;
    }
    return revealSettingsAnchor(root, ids);
  }, [nav]);

  const readPathB = () => {
    loadoutsGetState(getShownProfile())
      .then((s) => setPathB(s.path_b_active))
      .catch(() => {});
  };
  useEffect(() => readPathB(), []);
  useTauriEvent(subscribeLoadoutsChanged, readPathB);

  // Read the config of `profile`, the one Settings shows unless named.
  // Only the newest read lands, so a read of a profile Settings has left
  // never shows.
  const readsRef = useRef(0);
  const readConfig = useCallback((profile: string | undefined = getShownProfile()) => {
    const mine = ++readsRef.current;
    return fetchUiConfig(profile)
      .then((cfg) => {
        if (mine !== readsRef.current) return;
        setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
        setConfig(cfg);
        // The main window owns sending OS appearance flips. This window
        // follows them on its own listener.
        applyThemePrefs(cfg);
      })
      .catch((e) => {
        if (mine === readsRef.current) setError(String(e));
      });
  }, []);

  // Load current config and reveal the window once a frame with the
  // theme has gone out. The startup paint usually has it on screen
  // already.
  useEffect(() => {
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      void win.show().then(() => win.setFocus());
    };
    const fallback = window.setTimeout(reveal, 500);
    void readConfig().finally(() => showAfterThemePaint(reveal));
    return () => window.clearTimeout(fallback);
  }, [readConfig]);

  // A profile switch, #profile load, #profile reset, and an import each
  // replace the whole UI config of the profile in front. Read the one in
  // front again here, so the copy every page shows and edits is the new
  // one's. A login switch replaces it before the session list says the
  // session moved, so the read names no profile. While a page holds
  // another profile the replace is not this copy's, and Settings reads
  // again when it moves.
  useTauriEvent(subscribeUiConfigReplaced, () => {
    if (!isShownHeld()) void readConfig(undefined);
  });
  useEffect(() => subscribeShownMoves((profile) => void readConfig(profile)), [readConfig]);

  // Turning the theme scope global folds the custom themes of every
  // other profile into the shared list. Take the new list, since a later
  // edit of the custom themes sends the whole list, and one built on the
  // old list would drop them.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeProfilesChanged((changed) => {
      if (changed !== 'scope') return;
      fetchUiConfig(getShownProfile())
        .then((cfg) => {
          if (cancelled) return;
          setCustomThemes(cfg.custom_themes.map(customToAppTheme));
          setConfig((prev) => (prev ? { ...prev, custom_themes: cfg.custom_themes } : prev));
        })
        .catch((e) => setError(String(e)));
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Another window can change the theme (the palette's Choose theme).
  // subscribeThemeChanges repaints this window. While Switch themes is
  // off the id is your manual pick, so the config copy takes it and
  // Appearance shows it. While it follows the system or the game the id
  // is only the entry the OS or the game shows, and the theme fields
  // below carry the pick. This window's own save comes back too, so
  // while a save holds the theme fields the copy keeps what you picked.
  useTauriEvent(subscribeThemeChanges, (themeId) => {
    const current = configRef.current;
    if (!current || themeFollowOf(current) !== 'off') return;
    if (settingsSaveHolds(THEME_PREFS_FIELDS)) return;
    setConfig((prev) =>
      prev && themeFollowOf(prev) === 'off' && prev.theme !== themeId
        ? { ...prev, theme: themeId }
        : prev,
    );
  });

  // The seven theme fields another window saved. A palette pick while
  // Switch themes follows the system or the game fills a slot of its
  // pair, and the config copy takes it the same way.
  useTauriEvent(subscribeThemePrefs, (prefs) => {
    if (settingsSaveHolds(THEME_PREFS_FIELDS)) return;
    applyThemePrefs(prefs);
    setConfig((prev) => (prev ? { ...prev, ...prefs } : prev));
  });

  // The Affects pane menu picks a style or a marker in the main window,
  // and a profile switch brings the whole display, the tint and the
  // hours too. The config copy takes it, the way it takes a palette
  // theme pick, so Layout shows it. This window's own save comes back
  // too, so while a save holds a display field the copy keeps yours.
  useTauriEvent(subscribeAffectsDisplayChanged, (display) => {
    if (settingsSaveHolds(AFFECTS_DISPLAY_FIELDS)) return;
    setConfig((prev) =>
      prev && !sameAffectsDisplay(affectsDisplayOf(prev), display)
        ? { ...prev, ...affectsDisplayFields(display) }
        : prev,
    );
  });

  // The vitals menu picks a style or a Values form in the main window.
  // The config copy takes them, so Layout shows the pick, and keeps
  // yours while this window's own save holds them.
  useTauriEvent(subscribeVitalsOptionsChanged, (options) => {
    if (settingsSaveHolds(VITALS_MENU_FIELDS)) return;
    setConfig((prev) =>
      prev && (shownStyle(prev) !== options.style || prev.vitals_values !== options.values)
        ? { ...prev, ...vitalsStylePick(options.style), vitals_values: options.values }
        : prev,
    );
  });

  // The vitals text card saves your text in the main window. The config
  // copy takes it, so Customize vitals previews it, and keeps yours
  // while this window's own save holds it.
  useTauriEvent(subscribeVitalsTextChanged, (change) => {
    if (settingsSaveHolds(VITALS_TEXT_FIELDS)) return;
    setConfig((prev) =>
      prev &&
      (prev.vitals_text !== change.vitals_text ||
        prev.vitals_text_previous.join('\u0000') !== change.vitals_text_previous.join('\u0000'))
        ? { ...prev, ...change }
        : prev,
    );
  });

  // MUD text in Settings (patterns, commands, host and port) uses your
  // terminal font through --font-mud, the way the main window does.
  const fontFamily = config?.font_family;
  useEffect(() => {
    if (!fontFamily) return;
    const rendered = renderFontStack(fontFamily);
    loadFontStack(rendered);
    document.documentElement.style.setProperty('--app-font-family', rendered);
  }, [fontFamily]);

  const group = nav.target.group;
  const { Page, selfScroll: scrollsSelf } = PAGES[group];
  const selfScroll = typeof scrollsSelf === 'function' ? scrollsSelf(nav.target) : scrollsSelf;
  // A page inside the group, like General's session logs, adds a step
  // to the breadcrumb, and the group's name there leads back.
  const subpage = settingsSubpage(nav.target);

  return (
    <div className="st-app">
      <Sidebar group={group} onNavigate={go} pathB={pathB} mac={mac} />
      <main className="st-main">
        <header className="st-header" data-tauri-drag-region="">
          <div className="st-crumb" data-tauri-drag-region="">
            <span className="st-crumb-root" data-tauri-drag-region="">
              Settings
            </span>
            <ChevronRightIcon size={12} className="st-crumb-sep" />
            {subpage !== null && (
              <>
                <a
                  href={`#${group}`}
                  className="st-crumb-link"
                  onClick={(e) => {
                    e.preventDefault();
                    go({ group });
                  }}
                >
                  {settingsGroupLabel(group)}
                </a>
                <ChevronRightIcon size={12} className="st-crumb-sep" />
              </>
            )}
            <h1 className="st-crumb-title" data-tauri-drag-region="">
              {subpage ?? settingsGroupLabel(group)}
            </h1>
          </div>
          <ShownSession />
          {!mac && <WindowControls />}
        </header>
        <div ref={contentRef} className="st-content" data-scroll={selfScroll ? 'self' : undefined}>
          {error && (
            <p className="st-error" role="alert">
              {error}
            </p>
          )}
          <Page
            key={group}
            target={nav.target}
            navSeq={nav.seq}
            config={config}
            setConfig={setConfig}
            onError={setError}
            pathB={pathB}
            navigate={go}
            setLeaveGuard={setLeaveGuard}
          />
        </div>
      </main>
    </div>
  );
}
