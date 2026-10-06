import { keepFit } from '../../theme/appearanceSettings';
import { fitOffThread } from '../../theme/fitOffThread';
import { fitKey, needsFit } from '../../theme/gameFit';
import type { CustomTheme } from '../../ipc/theme';
import { customToAppTheme, setCustomThemes } from '../../theme/themes';
import type { UpdateConfig } from '../useSettingsAutoSave';

// The fits this window has asked for and not had back, by theme id and
// the colors fitted, so a theme asked for again before its answer fits
// once.
const fitting = new Set<string>();

/** Fit a custom theme's game colors off the main thread and keep the
 *  fit with the theme. The answer lands on the latest config, after the
 *  page that asked has closed too, as long as the theme still has the
 *  colors it was fitted to. A theme that passes every check needs no
 *  fit and is left as it is. */
export function fitAndKeep(theme: CustomTheme, update: UpdateConfig): void {
  const palette = customToAppTheme(theme).xterm;
  const key = `${theme.id} ${fitKey(palette)}`;
  if (fitting.has(key) || !needsFit(palette)) return;
  fitting.add(key);
  void fitOffThread(palette).then((fitted) => {
    fitting.delete(key);
    if (!fitted) return;
    update(
      (latest) => {
        const list = keepFit(latest.custom_themes, theme.id, palette, fitted);
        if (!list) return null;
        setCustomThemes(list.map(customToAppTheme));
        return { custom_themes: list };
      },
      { now: true },
    );
  });
}
