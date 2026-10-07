import { useEffect, useMemo, useState } from 'react';
import type { UiConfig } from '../../ipc/uiConfig';
import { vitalsSnapshotGet } from '../../ipc/vitals';
import { readPanelGameFace, usePanelFaceVersion } from '../../panel/panelFace';
import { panelFontFamily } from '../../panel/panelFont';
import { resolvePanelSize } from '../../panel/panelSize';
import { textCols } from '../../panel/vitalsTextFit';
import { useBandEnv } from '../../prompt/useBandEnv';
import type { BandEnv } from '../../terminal/bandCells';
import { nativeSurfaceEnabled } from '../../terminal/terminalRenderer';
import { resolveThemeTerminalColors } from '../../theme/themes';
import { galleryVitals, OFFLINE, type GalleryVitals } from './vitalsStyles';

// What the Style gallery and Customize vitals both read to draw your
// vitals as the panel does: your numbers, and the cells and colors a
// vitals text takes at a width.

/** The face a CSS font list draws in, for a canvas to measure: a
 *  `var()` read off the root, or the list itself. */
export function measurable(family: string): string {
  const name = /^var\((--[\w-]+)\)$/.exec(family)?.[1];
  if (!name || typeof document === 'undefined') return family;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || family;
}

/** The vitals the tiles draw, your live ones once the snapshot answers. */
export function useGalleryVitals(): GalleryVitals {
  const [data, setData] = useState<GalleryVitals>(OFFLINE);
  useEffect(() => {
    let open = true;
    vitalsSnapshotGet()
      .then((snapshot) => open && setData(galleryVitals(snapshot)))
      .catch(() => undefined);
    return () => {
      open = false;
    };
  }, []);
  return data;
}

/** How the panel draws your vitals text `width` px wide: the cells a
 *  row holds in your panel font and size, and the colors your pinned
 *  prompt takes. */
export function usePanelText(config: UiConfig, width: number): { cols: number; env: BandEnv } {
  const size = resolvePanelSize(config.panel_font_size, config.font_size);
  const family = panelFontFamily(config.panel_font);
  const faceVersion = usePanelFaceVersion();
  const cols = useMemo(
    () => textCols(width, family === null ? readPanelGameFace() : measurable(family), size),
    // faceVersion marks a face that loaded or changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [width, family, size, faceVersion],
  );
  const env = useBandEnv(
    resolveThemeTerminalColors(config.theme_terminal_colors),
    config.bright_bold,
    nativeSurfaceEnabled() ? 'native' : 'xterm',
    config.fit_game_colors,
  );
  return { cols, env };
}
