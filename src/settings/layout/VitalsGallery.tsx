import {
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { promptRenderMany, type PromptRendered } from '../../ipc/promptDesign';
import {
  shownStyle,
  VITALS_STYLES,
  vitalsOptionsOf,
  type UiConfig,
  type UiFields,
  type VitalsOptions,
  type VitalsStyle,
} from '../../ipc/uiConfig';
import { drawnVitalsText } from '../../ipc/vitals';
import { isMacPlatform } from '../../lib/shortcuts';
import { panelWidthFloor } from '../../panel/paneLayout';
import { panelWidthOf, usePanelLayout } from '../../panel/panelLayoutStore';
import {
  readPanelFace,
  readPanelGameFace,
  textWidth,
  usePanelFaceVersion,
} from '../../panel/panelFace';
import { panelFontFamily } from '../../panel/panelFont';
import { resolvePanelSize } from '../../panel/panelSize';
import { PaneTextSizeContext } from '../../panel/paneTextSize';
import { VitalsBlock } from '../../panel/VitalsFooter';
import { vitalsFitOf } from '../../panel/vitalsFit';
import { VitalsTextBlock } from '../../panel/VitalsText';
import { textRows, type TextLine } from '../../panel/vitalsTextFit';
import {
  shownRows,
  vitalInks,
  vitalsFlame,
  vitalsOn,
  vitalsStylePick,
  VITALS_STYLE_LABELS,
  type VitalInks,
} from '../../panel/vitalsView';
import type { MeasureText } from '../../panel/vitalsLedgerFit';
import type { VitalSample, Vitals } from '../../stores/gmcp/vitalsStore';
import type { BandEnv } from '../../terminal/bandCells';
import { playPalette, themeTokens } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';
import { arrowPick, galleryCaption, tileFit, type GalleryVitals } from './vitalsStyles';
import { measurable, useGalleryVitals, usePanelText } from './usePanelVitals';

// The Style gallery under Settings, Layout, Vitals (board 2 of the
// Vitals Styles review, Q17). Six tiles, two a row, as the theme
// gallery lays its tiles. Each tile draws the real footer of its style
// at your panel's width, in your panel font and size, with your
// Customize vitals choices, and scales it to the tile, down to three
// quarters. A panel too wide for that draws at the widest width that
// allows. The tiles leave your opponent out and keep their height, so
// the gallery never jumps in a fight.
//
// The numbers are your own, from the snapshot Settings asks for as it
// opens, or the prompt catalog's samples, 1020, 800 and 930, while no
// session has your vitals. The colors are the play palette worked out
// from your Fit game colors and Color vision. Settings plays the
// published palette everywhere else, so only the gallery shows the fit
// your panel draws.
//
// The pick rings in the accent, the arrow keys move it as in any radio
// group, and the caption under the tiles says what the picked style
// does. The style your 0.7 vitals grew into carries Yours in 0.7.

/** Your vitals text at `cols` cells with the gallery's numbers and no
 *  fight, or null until it renders. */
function useGalleryText(
  template: string,
  data: GalleryVitals,
  cols: number,
): PromptRendered | null {
  const [rendered, setRendered] = useState<PromptRendered | null>(null);
  useEffect(() => {
    let open = true;
    const { vitals } = data;
    const value = (cur: number, max: number) => (vitals.hidden ? '?' : `${cur}/${max}`);
    promptRenderMany([
      {
        template,
        values: data.live ? 'live' : 'sample',
        overrides: {
          values: {
            fight: false,
            hp: value(vitals.hp, vitals.maxhp),
            mana: value(vitals.mana, vitals.maxmana),
            move: value(vitals.move, vitals.maxmove),
            maxhp: String(vitals.maxhp),
            maxmana: String(vitals.maxmana),
            maxmove: String(vitals.maxmove),
          },
        },
        cols,
      },
    ])
      .then(([text]) => open && setRendered(text ?? null))
      .catch(() => open && setRendered(null));
    return () => {
      open = false;
    };
  }, [template, data, cols]);
  return rendered;
}

/** The width of a tile, measured on the one the ref lands on as
 *  Settings resizes. */
function useTileWidth(): [(el: HTMLSpanElement | null) => void, number] {
  const [tile, setTile] = useState<HTMLSpanElement | null>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    if (!tile || typeof ResizeObserver === 'undefined') return;
    const measure = () => setWidth(tile.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(tile);
    return () => observer.disconnect();
  }, [tile]);
  return [setTile, width];
}

export function VitalsGallery({
  config,
  onPick,
}: {
  config: UiConfig;
  onPick: (fields: UiFields) => void;
}) {
  const theme = useActiveTheme();
  const layout = usePanelLayout();
  const panel = Math.max(panelWidthFloor(isMacPlatform()), panelWidthOf(layout));
  const [tileRef, tile] = useTileWidth();
  const { width, scale } = tileFit(panel, tile);
  const size = resolvePanelSize(config.panel_font_size, config.font_size);
  const family = panelFontFamily(config.panel_font);
  const faceVersion = usePanelFaceVersion();
  const data = useGalleryVitals();
  const palette = useMemo(
    () => playPalette(theme, config.fit_game_colors, config.color_vision),
    [config.fit_game_colors, config.color_vision, theme],
  );
  const inks = useMemo(
    () => vitalInks(config.vitals_colors, palette, themeTokens(theme)),
    [config.vitals_colors, palette, theme],
  );
  const flame = useMemo(() => vitalsFlame(palette, themeTokens(theme)), [palette, theme]);
  const { cols, env } = usePanelText(config, width);
  const text = useGalleryText(drawnVitalsText(config), data, cols);
  const face = family === null ? readPanelFace() : measurable(family);
  const measure: MeasureText = (t, px, weight) =>
    textWidth(t, `${weight} ${px}px ${face}`, faceVersion);
  const gameFace = family === null ? readPanelGameFace() : measurable(family);
  const measureGame: MeasureText = (t, px, weight) =>
    textWidth(t, `${weight} ${px}px ${gameFace}`, faceVersion);
  return (
    <VitalsTiles
      config={config}
      vitals={data.vitals}
      history={data.history}
      text={text}
      env={env}
      inks={inks}
      flame={flame}
      panel={panel}
      width={width}
      scale={scale}
      size={size}
      family={family}
      measure={measure}
      measureGame={measureGame}
      tileRef={tileRef}
      onPick={onPick}
    />
  );
}

export interface VitalsTilesProps {
  config: UiConfig;
  vitals: Vitals;
  /** Your last Char.Vitals, oldest first, which the Traces tile draws. */
  history: readonly VitalSample[];
  /** Your vitals text rendered for the Text tile, or null before it is. */
  text: PromptRendered | null;
  env: BandEnv;
  inks: VitalInks;
  /** The color the Candles flame burns in. */
  flame: string;
  /** Your panel's width, and the width and scale each tile draws at. */
  panel: number;
  width: number;
  scale: number;
  /** Your panel size in px, and the Panel font as a CSS list, null for
   *  As designed. */
  size: number;
  family: string | null;
  measure: MeasureText;
  /** Measures in the game face, which Blocks draws in. */
  measureGame: MeasureText;
  /** Lands on the first tile, which the gallery measures. */
  tileRef?: ((el: HTMLSpanElement | null) => void) | undefined;
  onPick: (fields: UiFields) => void;
}

/** The gallery drawn from plain values, so a test draws every case. */
export function VitalsTiles({
  config,
  vitals,
  history,
  text,
  env,
  inks,
  flame,
  panel,
  width,
  scale,
  size,
  family,
  measure,
  measureGame,
  tileRef,
  onPick,
}: VitalsTilesProps) {
  const name = useId();
  const radios = useRef(new Map<VitalsStyle, HTMLInputElement>());
  const picked = shownStyle(config);
  const options = vitalsOptionsOf(config);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target;
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (!(target instanceof HTMLInputElement) || target.name !== name) return;
    const next = arrowPick(event.key, target.value as VitalsStyle);
    if (next === null) return;
    // Step here rather than leave it to the browser, so the pick and
    // the save follow the same order in every webview.
    event.preventDefault();
    onPick(vitalsStylePick(next));
    radios.current.get(next)?.focus();
  };

  const host = {
    width,
    transform: scale === 1 ? undefined : `scale(${scale})`,
    '--panel-text-px': String(size),
    ...(family === null
      ? {}
      : { fontFamily: family, '--font-panel': family, '--font-panel-game': family }),
  } as CSSProperties;

  const lines: TextLine[] = text
    ? textRows(text, []).map((row) => ({ left: row.cells, right: null }))
    : [];

  return (
    <>
      <fieldset className="st-gallery" data-st-anchor="style">
        <legend className="st-visually-hidden">Style</legend>
        <div className="st-vitals-grid" onKeyDown={onKeyDown}>
          {VITALS_STYLES.map((style) => (
            <label key={style} className="st-vitals-pick">
              <input
                ref={(el) => {
                  if (el) radios.current.set(style, el);
                  else radios.current.delete(style);
                }}
                type="radio"
                className="st-theme-input"
                name={name}
                value={style}
                checked={style === picked}
                onChange={() => onPick(vitalsStylePick(style))}
              />
              <span
                ref={style === VITALS_STYLES[0] ? tileRef : undefined}
                className="st-vitals-tile"
                aria-hidden="true"
              >
                <span className="st-vitals-tile-in panel-host" style={host}>
                  <PaneTextSizeContext.Provider value={size}>
                    {style === 'text' ? (
                      <VitalsTextBlock lines={lines} env={env} />
                    ) : (
                      <StyleTile
                        style={style}
                        vitals={vitals}
                        history={history}
                        options={{ ...options, style }}
                        inks={inks}
                        flame={flame}
                        width={width}
                        size={size}
                        measure={measure}
                        measureGame={measureGame}
                      />
                    )}
                  </PaneTextSizeContext.Provider>
                </span>
              </span>
              <span className="st-vitals-name">
                {VITALS_STYLE_LABELS[style]}
                {config.vitals_legacy_style === style && (
                  <span className="st-vitals-was">Yours in 0.7</span>
                )}
              </span>
            </label>
          ))}
        </div>
      </fieldset>
      <p className="st-meta st-theme-caption">{galleryCaption(picked, panel, width)}</p>
    </>
  );
}

/** One drawn style's footer, fitted to `width` as the panel fits it. */
function StyleTile({
  style,
  vitals,
  history,
  options,
  inks,
  flame,
  width,
  size,
  measure,
  measureGame,
}: {
  style: Exclude<VitalsStyle, 'text'>;
  vitals: Vitals;
  history: readonly VitalSample[];
  options: VitalsOptions;
  inks: VitalInks;
  flame: string;
  width: number;
  size: number;
  measure: MeasureText;
  measureGame: MeasureText;
}): ReactNode {
  const rows = shownRows(vitals, vitalsOn(options.order, options.off), options);
  const fit = vitalsFitOf(style, width, size, rows, null, options.values, measure, measureGame);
  return (
    <VitalsBlock
      vitals={vitals}
      history={history}
      combat={null}
      fit={fit}
      options={options}
      inks={inks}
      flame={flame}
    />
  );
}
