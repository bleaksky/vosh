import { useEffect, useRef } from 'react';
import { keepFit } from '../../../../lib/appearanceSettings';
import { fitOffThread } from '../../../../lib/fitOffThread';
import type { CustomTheme, UiConfig } from '../../../../lib/session';
import { customToAppTheme, setCustomThemes } from '../../../../lib/themes';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';

/** Fit a custom theme's game colors off the main thread and keep the
 *  fit with the theme. The answer lands only if the theme still has the
 *  colors it was fitted to and the page is still open. A theme left
 *  without a fit is fitted in play instead, once per launch. */
export function useKeepFit(
  config: UiConfig | null,
  update: UpdateConfig,
): (theme: CustomTheme) => void {
  const latest = useRef(config);
  const open = useRef(true);
  useEffect(() => {
    latest.current = config;
  }, [config]);
  useEffect(() => {
    open.current = true;
    return () => {
      open.current = false;
    };
  }, []);

  return (theme) => {
    const palette = customToAppTheme(theme).xterm;
    void fitOffThread(palette).then((fitted) => {
      const current = latest.current;
      if (!open.current || !current || !fitted) return;
      const list = keepFit(current.custom_themes, theme.id, palette, fitted);
      if (!list) return;
      setCustomThemes(list.map(customToAppTheme));
      update({ custom_themes: list }, { now: true });
    });
  };
}
