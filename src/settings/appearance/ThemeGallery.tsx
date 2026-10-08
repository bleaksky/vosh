import { useId, useRef, type KeyboardEvent } from 'react';
import { stepGalleryTheme } from '../../theme/appearanceSettings';
import type { Appearance } from '../../theme/chrome';
import { seenBy, type ColorVision } from '../../theme/gameFit';
import { themeThumb } from '../../theme/themeThumb';
import type { AppTheme } from '../../theme/themes';
import { Segmented } from '../../ui';

interface ThemeGalleryProps {
  /** Every theme in gallery order (galleryThemes). */
  themes: readonly AppTheme[];
  /** The theme Vosh shows now. Its radio is checked. */
  selected: string;
  onPick: (id: string) => void;
  /** Set while follow system appearance is on, to the appearance the
   *  OS asks for. The arrow keys then move only among themes of that
   *  appearance. */
  appearance?: Appearance | undefined;
  /** The color vision the tiles show, Typical for the colors as they
   *  are. With `onVision` the gallery draws the Vision switch above the
   *  tiles. */
  vision?: ColorVision | undefined;
  onVision?: ((vision: ColorVision) => void) | undefined;
}

/** The Vision switch, after board 9 of the Themes review. */
const VISIONS = [
  { value: 'typical', label: 'Typical' },
  { value: 'deuteranopia', label: 'Deuteranopia' },
  { value: 'protanopia', label: 'Protanopia' },
  { value: 'tritanopia', label: 'Tritanopia' },
] as const;

const STEPS: Readonly<Record<string, 1 | -1>> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/** The Theme gallery from the Appearance board: one radio per theme,
 *  drawn as a 90×59 thumbnail in the theme's own colors with its name
 *  under it. The arrow keys move the pick, as in any radio group, and
 *  focus goes with it. While follow system appearance is on they skip
 *  the themes of the other appearance, since a pick of one of those
 *  fills the other slot and leaves the theme on screen as it is.
 *
 *  The Vision switch above the tiles shows every tile as a player with
 *  that color vision sees it, through the matrices the game color fit
 *  measures with (gameFit seenBy), as board 9's Vision switch does. It
 *  only previews, and never changes a theme. */
export function ThemeGallery({
  themes,
  selected,
  onPick,
  appearance,
  vision = 'typical',
  onVision,
}: ThemeGalleryProps) {
  const name = useId();
  const visionLabel = useId();
  const see = (color: string) => seenBy(color, vision);
  const radios = useRef(new Map<string, HTMLInputElement>());

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = STEPS[event.key];
    const target = event.target;
    if (step === undefined || event.altKey || event.ctrlKey || event.metaKey) return;
    if (!(target instanceof HTMLInputElement) || target.name !== name) return;
    // The browser would check the next radio in the group, which may
    // be a theme of the other appearance. Step here instead.
    event.preventDefault();
    const next = stepGalleryTheme(themes, target.value, step, appearance);
    if (next === target.value) return;
    onPick(next);
    radios.current.get(next)?.focus();
  };

  return (
    <fieldset className="st-gallery">
      <legend className="visually-hidden">Theme</legend>
      {onVision && (
        <div className="st-gallery-bar">
          <span id={visionLabel} className="st-meta">
            Vision
          </span>
          <Segmented<ColorVision>
            options={VISIONS}
            value={vision}
            onChange={onVision}
            labelledBy={visionLabel}
          />
        </div>
      )}
      <div className="st-gallery-grid" onKeyDown={onKeyDown}>
        {themes.map((theme) => {
          const thumb = themeThumb(theme);
          return (
            <label key={theme.id} className="st-theme">
              <input
                ref={(el) => {
                  if (el) radios.current.set(theme.id, el);
                  else radios.current.delete(theme.id);
                }}
                type="radio"
                className="st-theme-input"
                name={name}
                value={theme.id}
                checked={theme.id === selected}
                onChange={() => onPick(theme.id)}
              />
              <span
                className="st-theme-tile"
                aria-hidden="true"
                style={{ background: see(thumb.bg) }}
              >
                <span className="st-theme-panel" style={{ background: see(thumb.panel) }} />
                <span className="st-theme-sep" style={{ background: see(thumb.sep) }} />
                <span className="st-theme-dot" style={{ background: see(thumb.accent) }} />
                <span
                  className="st-theme-bar st-theme-bar-1"
                  style={{ background: see(thumb.text) }}
                />
                <span
                  className="st-theme-bar st-theme-bar-2"
                  style={{ background: see(thumb.text) }}
                />
                <span
                  className="st-theme-bar st-theme-bar-3"
                  style={{ background: see(thumb.text) }}
                />
                <span
                  className="st-theme-ring"
                  style={{ boxShadow: `inset 0 0 0 1px ${thumb.ring}` }}
                />
              </span>
              <span className="st-theme-name">{theme.label}</span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
