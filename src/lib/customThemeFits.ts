import { fitOffThread } from './fitOffThread';
import { fitKey } from './gameFit';
import { customThemes, holdFit, onCustomThemesChanged } from './themes';

// The main window fits a custom theme that keeps no fit, once per
// launch, and holds the fit in memory (themes holdFit), so play draws it
// fitted. That covers a theme imported before Vosh kept fits, one Vosh
// 0.8.1 saved, which drops the fit, and one whose fit Settings closed
// before it could keep. The fit runs off the main thread, one theme at a
// time, and play draws the theme as published until it lands.

const tried = new Set<string>();
let busy = false;

function fitNext(): void {
  if (busy) return;
  const theme = customThemes().find((t) => !t.fitted && !tried.has(fitKey(t.xterm)));
  if (!theme) return;
  const palette = theme.xterm;
  tried.add(fitKey(palette));
  busy = true;
  void fitOffThread(palette).then((fitted) => {
    busy = false;
    if (fitted) holdFit(palette, fitted);
    fitNext();
  });
}

/** Fit the custom themes that keep no fit, now and whenever the list
 *  changes. Returns the call that stops listening. */
export function fitCustomThemes(): () => void {
  fitNext();
  return onCustomThemesChanged(fitNext);
}
