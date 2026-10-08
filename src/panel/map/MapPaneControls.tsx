import { useRef, useState } from 'react';
import { FLOOR_CHOICES, isResetView, resetView, type Floors, type Map3dView } from './map3dView';
import { MAP_STYLE_CHOICES, type MapStyle } from './mapStyle';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from '../../ui/MenuSurface';
import { returnToCommandLine } from '../paneActions';
import { CheckIcon } from '../../ui/icons';

// The map's own control in a panel pane: a small button in the drawing
// box's bottom right corner that shows while you point at the map or
// tab to it, and opens a menu of map styles, zoom, and the rows of the
// style you picked, the tileset in Tileset and the floors, sprites and
// view in 3D. At rest the pane shows only the drawing, as in the
// approved boards.

const MAP_STYLE_LABELS: Record<MapStyle, string> = {
  squares: 'Squares',
  glyphs: 'Glyphs',
  tileset: 'Tileset',
  '3d': '3D',
};

const FLOOR_LABELS: Record<Floors, string> = {
  yours: 'Your floor',
  adjacent: 'One floor up and down',
  all: 'Every floor',
};

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
  view3d: Map3dView;
  onView3d: (view: Map3dView) => void;
}

export function MapPaneControls(props: Props) {
  const { style, zoom, tilesetLoaded, view3d } = props;
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
          className="menu-narrow"
          anchor={anchor}
          at={{ x: rect.left, y: rect.bottom + 8, flipX: rect.right, flipY: rect.top - 8 }}
          onClose={close}
        >
          {MAP_STYLE_CHOICES.map((s) => (
            <MenuItem
              key={s}
              onSelect={run(() => props.onStyle(s))}
              trailing={s === style ? <CheckIcon className="menu-check" /> : null}
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
          {style === '3d' && (
            <>
              <MenuSeparator />
              {FLOOR_CHOICES.map((f) => (
                <MenuItem
                  key={f}
                  onSelect={run(() => props.onView3d({ ...view3d, floors: f }))}
                  trailing={f === view3d.floors ? <CheckIcon className="menu-check" /> : null}
                >
                  {FLOOR_LABELS[f]}
                </MenuItem>
              ))}
              <MenuSeparator />
              <MenuItem
                onSelect={run(() => props.onView3d({ ...view3d, sprites: !view3d.sprites }))}
                trailing={view3d.sprites ? <CheckIcon className="menu-check" /> : null}
              >
                Terrain sprites
              </MenuItem>
              <MenuItem
                disabled={isResetView(view3d)}
                onSelect={run(() => props.onView3d(resetView(view3d)))}
              >
                Reset view
              </MenuItem>
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
