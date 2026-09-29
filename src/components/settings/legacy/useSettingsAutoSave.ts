import { useEffect, useRef, useState } from 'react';
import {
  isOwnThemeEcho,
  setUiConfig,
  subscribeProfileSwitched,
  type UiConfig,
} from '../../../lib/session';
import { applyThemePrefs, subscribeThemeChanges, subscribeThemePrefs } from '../../../lib/theme';
import type { SetUiConfig } from '../pageTypes';

export interface AutoSaveOptions {
  /** Save on the next tick instead of after typing settles. Discrete
   *  picks that other windows show at once, like a theme, pass it. */
  now?: boolean;
}

/** Patch the window's config copy and save the whole snapshot. */
export type UpdateConfig = (patch: Partial<UiConfig>, options?: AutoSaveOptions) => void;

// Debounced auto-save shared by the config-backed editors. Text inputs
// can fire many updates in a row while the user types; the debounce
// coalesces them into one setUiConfig call after typing settles.
// setUiConfig owns every cross-window emit and dedupes against the
// previous snapshot, so callers only get the local theme refresh and
// the saved indicator.
export function useSettingsAutoSave(setConfig: SetUiConfig, onError: (e: string | null) => void) {
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  // The snapshot waiting on the debounce. A theme picked in another
  // window while it waits patches it, so the save does not put the old
  // theme back.
  const pendingRef = useRef<UiConfig | null>(null);
  const scheduleAutoSave = (next: UiConfig, delay: number) => {
    if (saveTimerRef.current) window.clearTimeout(saveTimerRef.current);
    pendingRef.current = next;
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      const cfg = pendingRef.current ?? next;
      pendingRef.current = null;
      void (async () => {
        try {
          await setUiConfig(cfg);
          applyThemePrefs(cfg);
          setSavedAt(Date.now());
        } catch (e) {
          onError(String(e));
        }
      })();
    }, delay);
  };
  const update: UpdateConfig = (patch, options = {}) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      scheduleAutoSave(next, options.now ? 0 : 250);
      return next;
    });
  };
  // A save still waiting on the debounce holds the previous profile's
  // snapshot. Drop it on a profile switch so it cannot land on the new
  // profile once SettingsApp has re-read the config.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeProfileSwitched(() => {
      if (saveTimerRef.current) window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
      pendingRef.current = null;
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);
  // The theme id another window applied is the manual pick only while
  // follow system appearance is off. This window's own save comes back
  // too, and is skipped.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeThemeChanges((themeId) => {
      const pending = pendingRef.current;
      if (!pending || pending.follow_system_appearance || isOwnThemeEcho(themeId)) return;
      pendingRef.current = { ...pending, theme: themeId };
    })
      .then((fn) => {
        if (cancelled) fn();
        else unsub = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);
  // The four theme fields another window saved, like a palette pick
  // that filled the light or dark entry while follow is on.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeThemePrefs((prefs) => {
      const pending = pendingRef.current;
      if (!pending || isOwnThemeEcho(prefs)) return;
      pendingRef.current = { ...pending, ...prefs };
    })
      .then((fn) => {
        if (cancelled) fn();
        else unsub = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);
  // Fade the "saved." indicator after 1.5s so it does not linger as
  // stale chrome long after the user actually saved.
  useEffect(() => {
    if (savedAt === null) return;
    const id = window.setTimeout(() => setSavedAt(null), 1500);
    return () => window.clearTimeout(id);
  }, [savedAt]);
  return { update, savedAt };
}
