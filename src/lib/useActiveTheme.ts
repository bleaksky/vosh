import { useSyncExternalStore } from 'react';
import { getCurrentThemeId } from './theme';
import { findTheme, type AppTheme } from './themes';

// The theme this window paints with, for a component that draws from
// its ANSI slots. applyTheme writes data-theme on the root on every
// apply, from this window, a broadcast, or a profile switch, so one
// observer hears them all. findTheme hands back the same object until
// the theme or the custom theme list changes, so the snapshot holds
// still between applies.

function subscribe(cb: () => void): () => void {
  const observer = new MutationObserver(cb);
  observer.observe(document.documentElement, { attributeFilter: ['data-theme'] });
  return () => observer.disconnect();
}

function snapshot(): AppTheme {
  return findTheme(getCurrentThemeId());
}

export function useActiveTheme(): AppTheme {
  return useSyncExternalStore(subscribe, snapshot);
}
