import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from 'react';
import type { PanelSide } from '../../ipc/uiConfig';
import { moveLeaf, type PaneLayout, type PaneTree } from '../../panel/paneLayout';
import { paneLabel } from '../../panel/paneTypes';
import { Button, Card, Section } from '../../ui';
import {
  SCHEMATIC_HEIGHT,
  SCHEMATIC_WIDTH,
  SIDEBAR_SCHEMATIC_WIDTH,
  VITALS_LINE,
  paneSchematic,
  sidebarSchematic,
  type SchematicRect,
} from './paneSchematic';
import {
  boxSide,
  dropAt,
  keyMove,
  movable,
  type SchematicBox,
  type SchematicDrop,
} from './schematicMove';

// Panel layout on the Characters page: the selected profile's sidebar
// and panel drawn from its real pane trees, side by side as the window
// holds them, beside a line about what Vosh saves and Reset to default.
// You move a pane on the drawing as in the window. Drag it over another
// pane or into an empty box, or focus it and press the arrow keys. Each
// move saves to the profile at once.

interface Props {
  /** The name the drawing's sentences use, like `Ilsabet`. */
  owner: string;
  panes: PaneLayout;
  /** The side the profile's panel takes. */
  panelSide: PanelSide;
  /** Save `next`, the layout after a move. */
  onMove: (next: PaneLayout) => void;
  onReset: () => void;
  resetting: boolean;
}

/** Between the two boxes, where the terminal sits in the window. */
const GAP = 12;
/** The box captions sit 16 under the boxes, in the label type. */
const CAPTION_Y = SCHEMATIC_HEIGHT + 16;
const HEIGHT = SCHEMATIC_HEIGHT + 20;
/** How far a press moves before it becomes a drag. */
const THRESHOLD = 3;
const HINT = 'Drag a pane here';

/** A press on a pane that may become a drag. */
interface Press {
  id: string;
  x: number;
  y: number;
  pointer: number;
  dragging: boolean;
}

export function PanelLayout({ owner, panes, panelSide, onMove, onReset, resetting }: Props) {
  const panel = useMemo(() => paneSchematic(panes.root, paneLabel, owner), [panes.root, owner]);
  const sidebar = useMemo(
    () => sidebarSchematic(panes.sidebar, paneLabel, owner),
    [panes.sidebar, owner],
  );
  const width = SIDEBAR_SCHEMATIC_WIDTH + GAP + SCHEMATIC_WIDTH;
  const boxes = useMemo<SchematicBox[]>(() => {
    const at = (tree: PaneTree, w: number) => (boxSide(tree, panelSide) === 'left' ? 0 : width - w);
    return [
      {
        tree: 'sidebar',
        x: at('sidebar', SIDEBAR_SCHEMATIC_WIDTH),
        bottom: SCHEMATIC_HEIGHT,
        drawing: sidebar,
      },
      { tree: 'panel', x: at('panel', SCHEMATIC_WIDTH), bottom: VITALS_LINE, drawing: panel },
    ];
  }, [panel, sidebar, panelSide, width]);

  const svgRef = useRef<SVGSVGElement | null>(null);
  const press = useRef<Press | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [target, setTarget] = useState<SchematicDrop | null>(null);
  // The same target, for a release that comes before the drawing does.
  const targetRef = useRef<SchematicDrop | null>(null);
  const aim = useCallback((drop: SchematicDrop | null) => {
    targetRef.current = drop;
    setTarget(drop);
  }, []);
  // The pane a key moved, which takes focus back once it draws again.
  const refocus = useRef<string | null>(null);
  // Each movable pane's element, by leaf id.
  const nodes = useRef(new Map<string, SVGGElement>());

  const local = (e: { clientX: number; clientY: number }) => {
    const box = svgRef.current?.getBoundingClientRect();
    return box ? { x: e.clientX - box.left, y: e.clientY - box.top } : { x: -1, y: -1 };
  };

  const stop = useCallback(() => {
    const was = press.current;
    press.current = null;
    if (was) svgRef.current?.releasePointerCapture?.(was.pointer);
    setDragging(null);
    aim(null);
  }, [aim]);

  // Esc puts a pane on its way back where it was.
  useEffect(() => {
    if (dragging === null) return;
    const key = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      stop();
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [dragging, stop]);

  useLayoutEffect(() => {
    const id = refocus.current;
    if (id === null) return;
    refocus.current = null;
    nodes.current.get(id)?.focus();
  });

  const down = (id: string) => (e: PointerEvent<SVGGElement>) => {
    if (e.button !== 0) return;
    press.current = { id, x: e.clientX, y: e.clientY, pointer: e.pointerId, dragging: false };
    svgRef.current?.setPointerCapture?.(e.pointerId);
  };

  const move = (e: PointerEvent<SVGSVGElement>) => {
    const p = press.current;
    if (!p) return;
    if (!p.dragging) {
      if (Math.hypot(e.clientX - p.x, e.clientY - p.y) < THRESHOLD) return;
      p.dragging = true;
      setDragging(p.id);
    }
    const at = local(e);
    aim(dropAt(boxes, at.x, at.y, p.id));
  };

  const up = () => {
    const p = press.current;
    const drop = targetRef.current;
    stop();
    if (p?.dragging && drop) onMove(moveLeaf(panes, p.id, drop.drop));
  };

  const key = (id: string) => (e: KeyboardEvent<SVGGElement>) => {
    const next = keyMove(panes, id, e.key, panelSide);
    if (next === null) return;
    e.preventDefault();
    refocus.current = id;
    onMove(next);
  };

  const drawBox = (box: SchematicBox) => {
    const { drawing, tree } = box;
    const empty = drawing.regions.length === 0;
    return (
      <g key={tree} transform={`translate(${box.x} 0)`} role="group" aria-label={drawing.ariaLabel}>
        <rect
          className="st-schematic-frame"
          x="0.5"
          y="0.5"
          width={drawing.width - 1}
          height={drawing.height - 1}
          rx="7.5"
        />
        <g clipPath={`url(#st-schematic-clip-${tree})`}>
          {drawing.regions.map((region) => {
            const canMove = movable(panes, region.id);
            return (
              <g
                key={region.id}
                data-leaf={region.id}
                ref={(el) => {
                  if (el) nodes.current.set(region.id, el);
                  else nodes.current.delete(region.id);
                }}
                className={`st-schematic-pane${canMove ? ' is-movable' : ''}${
                  dragging === region.id ? ' is-dragged' : ''
                }`}
                {...(canMove
                  ? {
                      role: 'button',
                      tabIndex: 0,
                      'aria-label': `${region.label}, in the ${tree}`,
                      'aria-describedby': 'st-layout-keys',
                      onPointerDown: down(region.id),
                      onKeyDown: key(region.id),
                    }
                  : {})}
              >
                <rect className="st-schematic-hit" {...rectAttrs(region.rect)} />
                {canMove && <rect className="st-schematic-ring" {...ringAttrs(region.rect)} />}
              </g>
            );
          })}
        </g>
        <path className="st-schematic-lines" d={drawing.lines} />
        {target?.drop.tree === tree && (
          <g
            className="st-schematic-drop"
            clipPath={`url(#st-schematic-clip-${tree})`}
            pointerEvents="none"
          >
            <rect className="st-schematic-wash" {...rectAttrs(target.wash, box.x)} />
            <rect className="st-schematic-line" {...rectAttrs(target.line, box.x)} />
          </g>
        )}
        {drawing.labels.map((label) => (
          <text
            key={`${label.text} ${label.x} ${label.y}`}
            className={`st-schematic-label${
              dragging !== null && label.leaf === dragging ? ' is-dragged' : ''
            }`}
            x={label.x}
            y={label.y}
          >
            {label.text}
          </text>
        ))}
        {empty && (
          <text className="st-schematic-label" x={10} y={16}>
            {HINT}
          </text>
        )}
        <text className="st-schematic-label st-schematic-caption" x={0} y={CAPTION_Y}>
          {tree === 'panel' ? 'Panel' : 'Sidebar'}
        </text>
      </g>
    );
  };

  return (
    <Section id="layout" title="Panel layout" card={false}>
      <Card className="st-layout">
        <svg
          ref={svgRef}
          className={`st-schematic${dragging !== null ? ' is-dragging' : ''}`}
          width={width}
          height={HEIGHT}
          viewBox={`0 0 ${width} ${HEIGHT}`}
          role="group"
          aria-label={`${sidebar.ariaLabel} ${panel.ariaLabel}`}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={stop}
        >
          <defs>
            {boxes.map((box) => (
              <clipPath key={box.tree} id={`st-schematic-clip-${box.tree}`}>
                <rect
                  x="1"
                  y="1"
                  width={box.drawing.width - 2}
                  height={box.drawing.height - 2}
                  rx="7"
                />
              </clipPath>
            ))}
          </defs>
          {boxes.map(drawBox)}
        </svg>
        <div className="st-layout-side">
          <p className="st-layout-note">
            Drag a pane to move it around, or over to the other side. Vosh keeps the panes you
            arrange for each character.
          </p>
          <p id="st-layout-keys" className="visually-hidden">
            Press Up or Down to move a pane past the next one, and Left or Right to move it between
            the sidebar and the panel.
          </p>
          <Button onClick={onReset} disabled={resetting}>
            Reset to default
          </Button>
        </div>
      </Card>
    </Section>
  );
}

/** A rect's attributes, moved left by `dx` into its box's own units. */
function rectAttrs(r: SchematicRect, dx = 0) {
  return { x: r.x - dx, y: r.y, width: r.width, height: r.height };
}

/** The focus ring of a region, inset 2 so its 2 px stroke shows whole
 *  inside the box's ring and the lines between panes. */
function ringAttrs(r: SchematicRect) {
  return {
    x: r.x + 2,
    y: r.y + 2,
    width: Math.max(0, r.width - 4),
    height: Math.max(0, r.height - 4),
    rx: 3,
  };
}
