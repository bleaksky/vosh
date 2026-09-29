import { useId } from 'react';
import { themeThumb } from '../../../../lib/themeThumb';
import type { AppTheme } from '../../../../lib/themes';

interface ThemeGalleryProps {
  /** Every theme in gallery order (galleryThemes). */
  themes: readonly AppTheme[];
  /** The theme Vosh shows now. Its radio is checked. */
  selected: string;
  onPick: (id: string) => void;
}

/** The Theme gallery from the Appearance board: one radio per theme,
 *  drawn as a 90×59 thumbnail in the theme's own colors with its name
 *  under it. The arrow keys move the pick, as in any radio group. */
export function ThemeGallery({ themes, selected, onPick }: ThemeGalleryProps) {
  const name = useId();
  return (
    <fieldset className="st-gallery">
      <legend className="st-visually-hidden">Theme</legend>
      <div className="st-gallery-grid">
        {themes.map((theme) => {
          const thumb = themeThumb(theme);
          return (
            <label key={theme.id} className="st-theme">
              <input
                type="radio"
                className="st-theme-input"
                name={name}
                value={theme.id}
                checked={theme.id === selected}
                onChange={() => onPick(theme.id)}
              />
              <span className="st-theme-tile" aria-hidden="true" style={{ background: thumb.bg }}>
                <span className="st-theme-panel" style={{ background: thumb.panel }} />
                <span className="st-theme-sep" style={{ background: thumb.sep }} />
                <span className="st-theme-dot" style={{ background: thumb.accent }} />
                <span className="st-theme-bar st-theme-bar-1" style={{ background: thumb.text }} />
                <span className="st-theme-bar st-theme-bar-2" style={{ background: thumb.text }} />
                <span className="st-theme-bar st-theme-bar-3" style={{ background: thumb.text }} />
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
