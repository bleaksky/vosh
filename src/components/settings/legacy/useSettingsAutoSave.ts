import { useEffect, useRef, useState } from 'react';
import { createDebouncedWrite, pendingWrites } from '../../../lib/pendingWrites';
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

/** One save waiting on the debounce, with the page that asked for it. */
interface AutoSave {
  cfg: UiConfig;
  saved: () => void;
  failed: (error: unknown) => void;
}

// Every page in the window saves through this one writer. Each save
// sends the whole config snapshot, built from the window's latest copy,
// so the newest snapshot holds every earlier edit. One writer sends only
// the newest, and an older snapshot can never land after it, which two
// writers flushed together on close could do. The save waiting on the
// debounce goes at once when the Settings window closes and when Vosh
// quits, through pendingWrites.
const autoSave = createDebouncedWrite<AutoSave>(async (job) => {
  try {
    await setUiConfig(job.cfg);
    applyThemePrefs(job.cfg);
    job.saved();
  } catch (e) {
    job.failed(e);
  }
});
pendingWrites.register(() => autoSave.flush());

// Debounced auto-save shared by the config-backed editors. Text inputs
// can fire many updates in a row while the user types; the debounce
// coalesces them into one setUiConfig call after typing settles.
// setUiConfig owns every cross-window emit and dedupes against the
// previous snapshot, so callers only get the local theme refresh and
// the saved indicator.
export function useSettingsAutoSave(setConfig: SetUiConfig, onError: (e: string | null) => void) {
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const onErrorRef = useRef(onError);
  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);
  const update: UpdateConfig = (patch, options = {}) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      autoSave.schedule(
        {
          cfg: next,
          saved: () => setSavedAt(Date.now()),
          failed: (e) => onErrorRef.current(String(e)),
        },
        options.now ? 0 : 250,
      );
      return next;
    });
  };
  // Leaving the page sends the waiting save at once too.
  useEffect(
    () => () => {
      void autoSave.flush();
    },
    [],
  );
  // A save still waiting on the debounce holds the previous profile's
  // snapshot. Drop it on a profile switch so it cannot land on the new
  // profile once SettingsApp has re-read the config.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeProfileSwitched(() => {
      autoSave.drop();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);
  // A theme picked in another window while a save waits patches it, so
  // the save does not put the old theme back. The theme id another
  // window applied is the manual pick only while follow system
  // appearance is off. This window's own save comes back too, and is
  // skipped.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeThemeChanges((themeId) => {
      if (isOwnThemeEcho(themeId)) return;
      autoSave.patch((job) =>
        job.cfg.follow_system_appearance ? job : { ...job, cfg: { ...job.cfg, theme: themeId } },
      );
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
      if (isOwnThemeEcho(prefs)) return;
      autoSave.patch((job) => ({ ...job, cfg: { ...job.cfg, ...prefs } }));
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
