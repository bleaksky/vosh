// The keyboard macros of the selected session's profile, kept current as
// you edit them, as their groups turn on and off, and as the selection
// moves to a session on another profile. The command line fires them,
// and the window's session keys ask whether a macro keeps the key.

import { useEffect, useMemo, useRef } from 'react';
import {
  listMacros,
  listMacroGroups,
  subscribeMacroGroupsChanged,
  subscribeMacrosChanged,
  type GroupToggle,
  type Macro,
} from '../ipc/automation';
import { subscribeProfileSwitched } from '../ipc/profiles';

export interface MacroKeys {
  /** The command a canonical key sends, if a macro that is on binds it. */
  command: (key: string) => string | undefined;
  /** Whether a macro that is on binds the canonical key. */
  bound: (key: string | null) => boolean;
}

/** The bound keys, each canonical key to the command it sends. */
export function useMacroKeys(): MacroKeys {
  // Keyboard macro bindings — keyed by canonical key string
  // ("F1", "Ctrl+N", "Numpad7"). Seeded from the backend and
  // refreshed on every macros-changed broadcast. Macros turned off
  // one by one, and macros whose group has been bulk-disabled, are
  // stripped here so a disabled "Combat" group means pressing F1 does
  // nothing (key bubbles back up as if no macro existed) instead
  // of firing a stale command.
  const macroMapRef = useRef<Map<string, string>>(new Map());
  const macroListRef = useRef<Macro[]>([]);
  const disabledMacroGroupsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    let cancelled = false;
    const unsubs: Array<() => void> = [];

    const rebuild = () => {
      const m = new Map<string, string>();
      for (const entry of macroListRef.current) {
        if (!entry.key || !entry.command) continue;
        if (entry.enabled === false) continue;
        if (entry.group && disabledMacroGroupsRef.current.has(entry.group)) continue;
        m.set(entry.key, entry.command);
      }
      macroMapRef.current = m;
    };

    const applyList = (list: Macro[]) => {
      macroListRef.current = list;
      rebuild();
    };

    const applyGroups = (groups: GroupToggle[]) => {
      const disabled = new Set<string>();
      for (const g of groups) {
        if (!g.enabled) disabled.add(g.name);
      }
      disabledMacroGroupsRef.current = disabled;
      rebuild();
    };

    const refreshGroups = () => {
      void listMacroGroups()
        .then((groups) => {
          if (!cancelled) applyGroups(groups);
        })
        .catch(() => {});
    };

    // Both lists name no profile, so they read the selected session's.
    const refresh = () => {
      listMacros()
        .then((list) => {
          if (!cancelled) applyList(list);
        })
        .catch(() => {});
      refreshGroups();
    };
    refresh();

    const hear = (subscribe: Promise<() => void>) => {
      void subscribe.then((fn) => {
        if (cancelled) fn();
        else unsubs.push(fn);
      });
    };

    hear(
      subscribeMacrosChanged((list) => {
        if (!cancelled) {
          applyList(list);
          // A new or removed macro can change which groups exist;
          // re-pull the group list so the disabled-set stays accurate.
          refreshGroups();
        }
      }),
    );

    // A group that turns can hand a key between your macro and a preset
    // macro in loadout mode, which turns the preset macro on or off, so
    // the list comes again with the groups.
    hear(
      subscribeMacroGroupsChanged(() => {
        if (!cancelled) refresh();
      }),
    );

    // A selection that crosses profiles sends the profile in front as a
    // switch, and the macros are that profile's.
    hear(
      subscribeProfileSwitched(() => {
        if (!cancelled) refresh();
      }),
    );

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);

  return useMemo(
    () => ({
      command: (key) => macroMapRef.current.get(key),
      bound: (key) => key !== null && macroMapRef.current.has(key),
    }),
    [],
  );
}
