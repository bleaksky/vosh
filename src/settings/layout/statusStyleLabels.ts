import type { StatusStyle } from '../../ipc/uiConfig';

/** The name Settings gives each status bar style. */
export const STATUS_STYLE_LABELS: Readonly<Record<StatusStyle, string>> = {
  meters: 'Meters',
  compact: 'Compact',
  strip: 'Strip',
  dashboard: 'Dashboard',
};
