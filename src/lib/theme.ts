// Drives the data-theme and data-prefers-contrast attributes on the
// root element. CSS in styles.css keys off both to apply the high
// contrast palette.

import type { ThemeChoice } from './session';

let cleanupContrastListener: (() => void) | null = null;

export function applyTheme(choice: ThemeChoice) {
  const root = document.documentElement;
  root.setAttribute('data-theme', choice);

  if (cleanupContrastListener) {
    cleanupContrastListener();
    cleanupContrastListener = null;
  }

  if (choice === 'system') {
    const mq = window.matchMedia('(prefers-contrast: more)');
    const update = () => {
      root.setAttribute('data-prefers-contrast', mq.matches ? 'more' : 'normal');
    };
    update();
    mq.addEventListener('change', update);
    cleanupContrastListener = () => mq.removeEventListener('change', update);
  } else {
    root.removeAttribute('data-prefers-contrast');
  }
}
