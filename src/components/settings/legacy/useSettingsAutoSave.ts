import { useEffect, useRef, useState } from 'react';
import { setUiConfig, subscribeProfileSwitched, type UiConfig } from '../../../lib/session';
import { applyTheme, subscribeThemeChanges } from '../../../lib/theme';
import type { SetUiConfig } from '../pageTypes';

// Debounced auto-save shared by the config-backed editors. Text inputs
// can fire many updates in a row while the user types; the debounce
// coalesces them into one setUiConfig call after typing settles.
// setUiConfig owns every cross-window emit and dedupes against the
// previous snapshot, so callers only get the local theme refresh and
// the saved indicator. Moved here unchanged from the old SettingsApp.
export function useSettingsAutoSave(setConfig: SetUiConfig, onError: (e: string | null) => void) {
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  // The snapshot waiting on the debounce. A theme picked in another
  // window while it waits patches it, so the save does not put the old
  // theme back.
  const pendingRef = useRef<UiConfig | null>(null);
  const scheduleAutoSave = (next: UiConfig) => {
    if (saveTimerRef.current) window.clearTimeout(saveTimerRef.current);
    pendingRef.current = next;
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      const cfg = pendingRef.current ?? next;
      pendingRef.current = null;
      void (async () => {
        try {
          await setUiConfig(cfg);
          applyTheme(cfg.theme);
          setSavedAt(Date.now());
        } catch (e) {
          onError(String(e));
        }
      })();
    }, 250);
  };
  const update = (patch: Partial<UiConfig>) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      scheduleAutoSave(next);
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
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeThemeChanges((themeId) => {
      if (pendingRef.current) pendingRef.current = { ...pendingRef.current, theme: themeId };
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
