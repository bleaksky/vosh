import { fitOffThread } from './fitOffThread';
import { fitKey, needsFit, type ColorVision } from './gameFit';
import type { UiConfig } from './session';
import { customThemes, holdFit, holdVisionFit, onMissingVisionFit, type AppTheme } from './themes';

// The main window fits a custom theme you play that keeps no fit and
// holds the fit in memory (themes holdFit), so play draws it fitted
// until Settings keeps one. That covers a theme imported before Vosh
// kept fits, one Vosh 0.8.1 saved, which drops the fit, and one whose
// fit Settings closed before it could keep. It reads the config this
// window loads, at launch and when a profile switch replaces it, and
// never the lists Settings sends, since Settings fits what you import
// or change and keeps it.

// The colors asked for this launch, so a theme fits once.
const asked = new Set<string>();

/** Fit, off the main thread, each custom theme that `cfg` names as your
 *  theme, your light theme or your dark theme and that keeps no fit,
 *  while Fit game colors is on. Play draws the theme as published until
 *  its fit lands. Call it after the custom themes of `cfg` are set. */
export function fitThemesInPlay(cfg: UiConfig): void {
  if (!cfg.fit_game_colors) return;
  const inPlay = new Set([cfg.theme, cfg.light_theme, cfg.dark_theme]);
  for (const theme of customThemes()) {
    const palette = theme.xterm;
    const key = fitKey(palette);
    if (!inPlay.has(theme.id) || theme.fitted || asked.has(key) || !needsFit(palette)) continue;
    asked.add(key);
    void fitOffThread(palette).then((fitted) => {
      if (fitted) holdFit(palette, fitted);
    });
  }
}

// The fits for a color vision asked this launch, by the vision and the
// colors, so a theme fits once for each vision.
const askedVision = new Set<string>();

/** Fit `theme` for `vision` off the main thread, once this launch, and
 *  hold the fit (themes holdVisionFit). Play asks through visionFitOf
 *  when it draws a custom theme for a vision this window holds no fit
 *  of, and draws the Typical fit until this one lands. The fit starts
 *  from the Typical fit the theme keeps, when it keeps one. */
export function fitForVision(theme: AppTheme, vision: ColorVision): void {
  const palette = theme.xterm;
  const key = `${vision} ${fitKey(palette)}`;
  if (vision === 'typical' || askedVision.has(key)) return;
  askedVision.add(key);
  // A palette that holds every floor the vision raises as published
  // plays as published. Play may be drawing, so the list changes later.
  if (!needsFit(palette, vision)) {
    queueMicrotask(() => holdVisionFit(palette, vision, {}));
    return;
  }
  void fitOffThread(palette, vision, theme.fitted).then((fitted) => {
    if (fitted) holdVisionFit(palette, vision, fitted);
  });
}

onMissingVisionFit(fitForVision);
