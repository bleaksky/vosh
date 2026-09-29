import { useRef, useState } from 'react';
import type { MapStyle } from '../ServerMapView';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from './MenuSurface';
import { returnToCommandLine } from './paneActions';
import { CheckIcon } from './paneIcons';

// The map's own control in a panel pane: a small button in the drawing
// box's bottom right corner that shows while you point at the map or
// tab to it, and opens a menu of map styles, zoom, and the tileset.
// At rest the pane shows only the drawing, as in the approved boards.

const MAP_STYLE_LABELS: Record<MapStyle, string> = {
  plain: 'Plain',
  squares: 'Squares',
  glyphs: 'Glyphs',
  tileset: 'Tileset',
};

const STYLES: MapStyle[] = ['plain', 'squares', 'glyphs', 'tileset'];

interface Props {
  style: MapStyle;
  onStyle: (style: MapStyle) => void;
  zoom: number;
  canZoomIn: boolean;
  canZoomOut: boolean;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomReset: () => void;
  tilesetLoaded: boolean;
  onLoadTileset: () => void;
  onClearTileset: () => void;
}

export function MapPaneControls(props: Props) {
  const { style, zoom, tilesetLoaded } = props;
  const ref = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const anchor = ref.current;
  const rect = open && anchor ? anchor.getBoundingClientRect() : null;

  const close = (reason: MenuCloseReason | 'select') => {
    setOpen(false);
    if (reason !== 'outside') returnToCommandLine();
  };
  const run = (action: () => void) => () => {
    close('select');
    action();
  };

  return (
    <>
      <button
        ref={ref}
        type="button"
        className="pane-map-tools"
        aria-label="Map style and zoom"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <SlidersIcon />
      </button>
      {open && anchor && rect && (
        <MenuSurface
          label="Map style and zoom"
          className="pane-menu-narrow"
          anchor={anchor}
          at={{ x: rect.left, y: rect.bottom + 8, flipX: rect.right, flipY: rect.top - 8 }}
          onClose={close}
        >
          {STYLES.map((s) => (
            <MenuItem
              key={s}
              onSelect={run(() => props.onStyle(s))}
              trailing={s === style ? <CheckIcon className="pane-menu-check" /> : null}
            >
              {MAP_STYLE_LABELS[s]}
            </MenuItem>
          ))}
          <MenuSeparator />
          {/* Zoom rows keep the menu open so you can step more than once. */}
          <MenuItem disabled={!props.canZoomIn} onSelect={props.onZoomIn}>
            Zoom in
          </MenuItem>
          <MenuItem disabled={!props.canZoomOut} onSelect={props.onZoomOut}>
            Zoom out
          </MenuItem>
          <MenuItem
            disabled={Math.abs(zoom - 1) < 1e-6}
            onSelect={props.onZoomReset}
            trailing={<span className="pane-map-zoom">{Math.round(zoom * 100)}%</span>}
          >
            Actual size
          </MenuItem>
          {style === 'tileset' && (
            <>
              <MenuSeparator />
              <MenuItem onSelect={run(props.onLoadTileset)}>Load tileset…</MenuItem>
              {tilesetLoaded && (
                <MenuItem onSelect={run(props.onClearTileset)}>Clear tileset</MenuItem>
              )}
            </>
          )}
        </MenuSurface>
      )}
    </>
  );
}

function SlidersIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.25"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M2.5 5.5h11M2.5 10.5h11M10.5 3.5v4M5.5 8.5v4" />
    </svg>
  );
}
