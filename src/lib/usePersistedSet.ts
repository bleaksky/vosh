import { useCallback, useState } from 'react';

// Collapse-state persistence for settings group sections (macros,
// triggers). The set of collapsed group names survives tab switches and
// relaunches via localStorage, so a long list stays folded the way you
// left it instead of re-expanding on every visit.
export function usePersistedSet(storageKey: string): [Set<string>, (key: string) => void] {
  const [set, setSet] = useState<Set<string>>(() => {
    try {
      const raw = localStorage.getItem(storageKey);
      const arr: unknown = raw ? JSON.parse(raw) : [];
      return new Set(
        Array.isArray(arr) ? arr.filter((v): v is string => typeof v === 'string') : [],
      );
    } catch {
      return new Set();
    }
  });
  const toggle = useCallback(
    (key: string) => {
      setSet((prev) => {
        const next = new Set(prev);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        try {
          localStorage.setItem(storageKey, JSON.stringify([...next]));
        } catch {
          // storage full or unavailable; collapse still works this session
        }
        return next;
      });
    },
    [storageKey],
  );
  return [set, toggle];
}
