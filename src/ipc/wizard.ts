// The shared catalog wizard, which previews and applies the move to
// loadouts.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { MIGRATION_APPLIED } from './events';

// Path B migration preview. The backend walks the current profile set,
// loads each per-profile snapshot, and returns the merge plan: every
// auto-resolved item, every conflict (one entry per name with two or
// more diverging variants), and one derived loadout per source profile.
// Read-only — nothing is written to disk by this call. The eventual
// migration_apply (not in this build) commits the plan after the user
// picks conflict winners in the wizard.

/** A conflict of `preset` kind holds your edits to the preset its name
 *  gives, one version for each profile that edits it its own way. */
export type MigrationItemKind = 'alias' | 'trigger' | 'macro' | 'preset';

export interface MigrationVariant {
  source_profile: string;
  /** Whether its profile had this copy switched on. */
  switched_on: boolean;
  item: { kind: MigrationItemKind; item: Record<string, unknown> };
}

export interface MigrationConflict {
  kind: MigrationItemKind;
  name: string;
  variants: MigrationVariant[];
  /** The profile whose version the wizard keeps unless you pick another:
   *  the one version switched on anywhere when exactly one is, or else
   *  the first profile. */
  default_source: string;
}

export interface MigrationLoadoutPreview {
  name: string;
  description?: string | null;
  enabled_groups: string[];
}

export interface MigrationPlan {
  source_profiles: string[];
  auto_resolved: {
    aliases: Array<{ name: string; group?: string | null }>;
    triggers: Array<{ name: string; group?: string | null }>;
    macros: Array<{ key: string; group?: string | null }>;
  };
  conflicts: MigrationConflict[];
  loadouts: MigrationLoadoutPreview[];
  /** The enabled preset list every character shares in loadout mode. */
  shared_presets: string[];
  /** Each source profile's own enabled preset list, in the order of
   *  source_profiles. A profile that never saved a file holds the
   *  defaults, the empty list. */
  profile_presets: string[][];
}

/** Preview the shared catalog. `library` holds the id of every preset in
 *  the library this build installs from, so a preset it no longer has
 *  stays off for characters whose file lacks it. */
export async function migrationAnalyze(library: string[]): Promise<MigrationPlan> {
  return invoke('migration_analyze', { library });
}

export interface MigrationConflictResolution {
  kind: MigrationItemKind;
  name: string;
  source_profile: string;
}

// Commit the migration. The backend writes catalog.toml + loadouts.toml
// and moves per-profile files into profiles/legacy/. Returns Ok once
// the files have landed. The runtime stays in legacy mode until the
// user relaunches Vosh; the startup hook detects catalog.toml on next
// launch and enters Path B mode. The wizard prompts the user to quit
// + reopen via appQuit because app.restart() is fragile in dev mode
// and silently leaves the WebView with no frontend to load.
export async function migrationApply(
  resolutions: MigrationConflictResolution[],
  library: string[],
): Promise<void> {
  return invoke('migration_apply', { resolutions, library });
}

/** Hear that the shared catalog wizard wrote its files. Nothing saves
 *  until Vosh opens again, and the main window says so. */
export async function subscribeMigrationApplied(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>(MIGRATION_APPLIED, () => cb());
}
