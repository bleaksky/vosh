import {
  MenuItem,
  MenuSeparator,
  MenuSurface,
  type MenuCloseReason,
  type MenuPlacement,
} from '../../ui/MenuSurface';
import { revealLabel } from '../../lib/revealLabel';

// The more menu of a plugin row (board 4), opened as the profile menu
// opens in Characters. A press on the row opens the plugin, so the menu
// has no Open. Remove sits under a separator and asks first.

interface Props {
  name: string;
  at: MenuPlacement;
  /** The more button that opened it, whose own press toggles it. */
  anchor: HTMLButtonElement | null;
  onClose: (reason: MenuCloseReason) => void;
  onReload: () => void;
  onReveal: () => void;
  onExport: () => void;
  onRemove: () => void;
}

export function PluginMenu({
  name,
  at,
  anchor,
  onClose,
  onReload,
  onReveal,
  onExport,
  onRemove,
}: Props) {
  const platform =
    typeof document === 'undefined' ? undefined : document.documentElement.dataset.platform;
  return (
    <MenuSurface label={`${name} options`} anchor={anchor} at={at} onClose={onClose}>
      <MenuItem onSelect={onReload}>Reload</MenuItem>
      <MenuItem onSelect={onReveal}>{revealLabel(platform)}</MenuItem>
      <MenuItem onSelect={onExport}>Export to Downloads</MenuItem>
      <MenuSeparator />
      <MenuItem onSelect={onRemove}>Remove…</MenuItem>
    </MenuSurface>
  );
}
