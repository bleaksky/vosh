import {
  AFFECTS_MARKERS,
  AFFECTS_STYLES,
  type AffectsDisplay,
  type AffectsMarker,
  type AffectsStyle,
} from './session';

// The names Settings, Layout, Affects and the Affects pane menu give
// the pane's styles and markers, and the menu's choices. Pure, so both
// read the same words and the menu's checks are unit tested.

export const AFFECTS_STYLE_LABELS: Readonly<Record<AffectsStyle, string>> = {
  timers: 'Timers first',
  countdown: 'Countdown',
  chips: 'Grouped chips',
};

export const AFFECTS_MARKER_LABELS: Readonly<Record<AffectsMarker, string>> = {
  dot: 'Dot',
  square: 'Square',
  plus_minus: 'Plus and minus',
  none: 'None',
};

/** One row of a pane menu choice list, checked on the current pick. */
export interface MenuChoice<T extends string> {
  value: T;
  label: string;
  checked: boolean;
}

export function affectsStyleChoices(display: AffectsDisplay): MenuChoice<AffectsStyle>[] {
  return AFFECTS_STYLES.map((value) => ({
    value,
    label: AFFECTS_STYLE_LABELS[value],
    checked: value === display.style,
  }));
}

export function affectsMarkerChoices(display: AffectsDisplay): MenuChoice<AffectsMarker>[] {
  return AFFECTS_MARKERS.map((value) => ({
    value,
    label: AFFECTS_MARKER_LABELS[value],
    checked: value === display.marker,
  }));
}

/** Grouped chips show the state on each chip and draw no marker, so
 *  the marker applies only to Timers first and Countdown. */
export function markerApplies(display: AffectsDisplay): boolean {
  return display.style !== 'chips';
}

/** The submenus of a pane's more menu. */
export type PaneSubmenu = 'show' | 'style' | 'marker';

/** The open submenu, and whether it opened from the keyboard and so
 *  takes focus. */
export interface PaneSubmenuState {
  which: PaneSubmenu;
  focus: boolean;
}

/** Open `which`, which closes any other. Pointing at the row of a
 *  submenu the keyboard already opened leaves it as it is, so its focus
 *  stays where the arrow keys put it. */
export function openPaneSubmenu(
  prev: PaneSubmenuState | null,
  which: PaneSubmenu,
  focus: boolean,
): PaneSubmenuState {
  if (prev && prev.which === which && !focus) return prev;
  return { which, focus };
}
