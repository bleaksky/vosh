import { fitOffThread } from './fitOffThread';
import { fitKey, needsFit, type ColorVision } from './gameFit';
import type { UiConfig } from './session';
import {
  customThemes,
  holdFit,
  holdVisionFit,
  onMissingVisionFit,
  type AppTheme,
  type XtermPalette,
} from './themes';

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

// The swaps for a color vision asked this launch, by the vision, the
// colors and the slots Typical plays over them, so a theme swaps once
// for each.
const askedVision = new Set<string>();

/** Swap `theme` for `vision` off the main thread, from `start`, the
 *  slots Typical plays over its published palette, once this launch, and
 *  hold the swap (themes holdVisionFit). Play asks through visionFitOf
 *  when it draws a custom theme for a vision this window holds no swap
 *  of, and draws the Typical slots until this one lands. */
export function fitForVision(
  theme: AppTheme,
  vision: ColorVision,
  start: Partial<XtermPalette> = {},
): void {
  const palette = theme.xterm;
  const key = `${vision} ${fitKey(palette)} from ${fitKey({ ...palette, ...start })}`;
  if (vision === 'typical' || askedVision.has(key)) return;
  askedVision.add(key);
  void fitOffThread(palette, vision, start).then((swapped) => {
    if (swapped) holdVisionFit(palette, vision, start, swapped);
  });
}

onMissingVisionFit(fitForVision);
