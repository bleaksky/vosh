import { useEffect, useRef, useState } from 'react';
import { createDebouncedWrite, pendingWrites } from '../lib/pendingWrites';
import { THEME_PREFS_FIELDS } from '../ipc/theme';
import { setUiFields, type UiConfig, type UiFields } from '../ipc/uiConfig';
import { broadcastUiConfigChanges } from '../ipc/uiConfigBroadcast';
import { playsProfile, profileInFront } from '../stores/session/sessionsStore';
import { applyThemePrefs, getThemePrefs } from '../theme/theme';
import type { SetUiConfig } from './pageTypes';
import { getShownProfile } from './shownProfile';

export interface AutoSaveOptions {
  /** Save on the next tick instead of after typing settles. Discrete
   *  picks that other windows show at once, like a theme, pass it. */
  now?: boolean;
}

/** Patch the window's config copy and save the fields the patch names.
 *  A patch can also be worked out from the latest copy, for an answer
 *  that lands after the page that asked for it closed. Null leaves the
 *  copy as it is. */
export type UpdateConfig = (
  patch: UiFields | ((latest: UiConfig) => UiFields | null),
  options?: AutoSaveOptions,
) => void;

/** One save waiting on the debounce, with the page that asked for it. */
interface AutoSave {
  /** The profile Settings showed as you made the edits, which the save
   *  names, or undefined for the selected session's. */
  profile: string | undefined;
  /** Every patch you made while the save waited, merged. */
  fields: UiFields;
  /** What those fields held before your first edit in this save. */
  before: UiFields;
  /** The config copy as of your last edit. */
  after: UiConfig;
  saved: () => void;
  failed: (error: unknown) => void;
}

/** The fields the shown theme comes from, the seven that pick it and
 *  the custom themes that can draw it. */
const THEME_FIELDS = [...THEME_PREFS_FIELDS, 'custom_themes'] as const;

/** The saves on their way to the backend. A pick sent at once can go
 *  while an earlier save still waits on its answer. */
const sending = new Set<AutoSave>();

/** Whether `profile` is `other`, as far as this window knows. A
 *  profile left out names the selected session's, or one the session
 *  list has not named yet. */
function sameProfile(profile: string | undefined, other: string | null | undefined): boolean {
  return profile === undefined || other == null || profile === other;
}

// Every page in the window saves through this one writer. An edit made
// while a save waits merges into it, so one save carries every edit and
// an earlier value of a field never lands after a later one. Each save
// names the profile it was made on, so it lands there even when Settings
// moved to another profile while it waited. The save waiting on the
// debounce goes at once when the Settings window closes and when Vosh
// quits, through pendingWrites.
const autoSave = createDebouncedWrite<AutoSave>(async (job) => {
  // A profile no session plays any more has left memory, and the app
  // turns a save to it away. Drop the save, and the fields it held.
  if (!playsProfile(job.profile)) return;
  sending.add(job);
  try {
    await setUiFields(job.fields, job.profile);
    // The other windows show the profile in front, so a save to a
    // profile behind tells them nothing.
    if (sameProfile(job.profile, profileInFront())) {
      await broadcastUiConfigChanges(job.after, { ...job.after, ...job.before });
    }
    // Repaint Settings so an edited custom theme shows. A save of the
    // theme fields shows them, since Settings keeps its own while the
    // save holds them. A save of the custom themes alone shows the
    // theme Settings shows now, since its copy can be older than a
    // theme the palette picked since. A save to a profile Settings has
    // left shows nothing.
    const shown = sameProfile(job.profile, getShownProfile());
    if (shown && THEME_FIELDS.some((field) => field in job.fields)) {
      const ownPrefs = THEME_PREFS_FIELDS.some((field) => field in job.fields);
      applyThemePrefs(ownPrefs ? job.after : (getThemePrefs() ?? job.after));
    }
    job.saved();
  } catch (e) {
    job.failed(e);
  } finally {
    sending.delete(job);
  }
});
pendingWrites.register(() => autoSave.flush());

/** Whether the save waiting or a save on its way holds any of `fields`.
 *  Settings hears its own broadcasts too, and one can carry a value
 *  older than a pick you made while its save ran, so the window keeps
 *  its own value of a field a save holds. */
export function settingsSaveHolds(fields: readonly (keyof UiFields)[]): boolean {
  const saves = [autoSave.waiting(), ...sending];
  return saves.some((save) => save !== null && fields.some((field) => field in save.fields));
}

/** Queue `change`, made on the copy `prev` of `profile`'s config, on the
 *  window's writer, and return the copy after it. */
export function queueSettingsChange(
  prev: UiConfig,
  change: UiFields,
  delayMs: number,
  report: Pick<AutoSave, 'saved' | 'failed'>,
  profile?: string,
): UiConfig {
  const after = { ...prev, ...change };
  const before: UiFields = Object.fromEntries(
    Object.keys(change).map((field) => [field, prev[field as keyof UiFields]]),
  );
  // A save waiting for another profile goes first, so the edits of two
  // profiles never merge into one save.
  const other = autoSave.waiting();
  if (other && other.profile !== profile) void autoSave.flush();
  // Merging the same change twice gives the same save, since React can
  // run a state updater twice.
  autoSave.schedule(
    (waiting) => ({
      profile,
      fields: { ...waiting?.fields, ...change },
      before: { ...before, ...waiting?.before },
      after,
      ...report,
    }),
    delayMs,
  );
  return after;
}

// Debounced auto-save shared by the config-backed editors. A text field
// can fire many updates in a row while you type, and the debounce
// gathers them into one save once typing settles. The save tells every
// other window what changed, so a page only shows the saved indicator.
export function useSettingsAutoSave(setConfig: SetUiConfig, onError: (e: string | null) => void) {
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const onErrorRef = useRef(onError);
  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);
  const update: UpdateConfig = (patch, options = {}) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const change = typeof patch === 'function' ? patch(prev) : patch;
      if (!change) return prev;
      return queueSettingsChange(
        prev,
        change,
        options.now ? 0 : 250,
        {
          saved: () => setSavedAt(Date.now()),
          failed: (e) => onErrorRef.current(String(e)),
        },
        getShownProfile(),
      );
    });
  };
  // Leaving the page sends the waiting save at once too.
  useEffect(
    () => () => {
      void autoSave.flush();
    },
    [],
  );
  // Fade the "saved." indicator after 1.5s so it does not linger as
  // stale chrome long after the user actually saved.
  useEffect(() => {
    if (savedAt === null) return;
    const id = window.setTimeout(() => setSavedAt(null), 1500);
    return () => window.clearTimeout(id);
  }, [savedAt]);
  return { update, savedAt };
}
