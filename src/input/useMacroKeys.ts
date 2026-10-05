// The keyboard macros the command line fires, kept current as you edit
// them and as their groups turn on and off.

import { useEffect, useRef } from 'react';
import {
  listMacros,
  listMacroGroups,
  subscribeMacroGroupsChanged,
  subscribeMacrosChanged,
  type GroupState,
  type Macro,
} from '../ipc/automation';

/** The bound keys, each canonical key to the command it sends. */
export function useMacroKeys() {
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

    const applyGroups = (groups: GroupState[]) => {
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

    listMacros()
      .then((list) => {
        if (!cancelled) applyList(list);
      })
      .catch(() => {});
    refreshGroups();

    subscribeMacrosChanged((list) => {
      if (!cancelled) {
        applyList(list);
        // A new or removed macro can change which groups exist;
        // re-pull the group list so the disabled-set stays accurate.
        refreshGroups();
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });

    subscribeMacroGroupsChanged(() => {
      if (!cancelled) refreshGroups();
    }).then((fn) => {
      if (cancelled) fn();
      else unsubs.push(fn);
    });

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);

  return macroMapRef;
}
