import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent,
  type RefObject,
} from 'react';
import { PANE_TYPES, setWeights, type PaneType } from '../../lib/paneLayout';
import { AffectsPane } from './AffectsPane';
import { ChatPane } from './ChatPane';
import { GroupPane } from './GroupPane';
import { ImmPane } from './ImmPane';
import { MapPane } from './MapPane';
import { PaneLeafContext } from './paneActions';
import { PANE_MIN_H, dragSizes, layoutPanes, type HandleBox } from './paneGeometry';
import { getPanelLayout, setPaneTree, usePanelLayout } from './panelLayoutStore';
import { PANE_LABELS } from './paneTypes';
import { usePaneMins } from './usePaneMins';
import { VitalsFooter } from './VitalsFooter';

// The right-hand panel (SPEC 9): the active profile's pane tree from
// the title band down, then the vitals pinned at the bottom. The lines
// between panes are handles you drag to share the space. The shell
// owns the panel's column, its left edge drag, and its label.
//
// Every pane renders as a flat, absolutely placed sibling keyed by its
// pane type, which the tree holds at most once. Splitting, closing, or
// showing a pane somewhere else only moves boxes, so the map canvas
// and each pane's scroll position survive every tree edit, and a pane
// moved with Show here instead keeps its state too.
//
// No pane drops below the height it reads at while the panel has room
// (PANE_MIN_H, raised for Affects and Group to hold your tracked
// affects, anything harmful, and every member). On a panel too short
// for every pane, the lightest ones come up short and scroll inside
// their box, header and all for the map, whose drawing has no list of
// its own to scroll.

// Arrow keys move a focused handle this far, Shift for bigger steps.
const KEY_STEP = 8;
const KEY_STEP_BIG = 32;

const PANES: Record<PaneType, () => React.ReactNode> = {
  map: () => <MapPane />,
  affects: () => <AffectsPane />,
  group: () => <GroupPane />,
  chat: () => <ChatPane />,
  imm: () => <ImmPane />,
};

export function PanelHost() {
  const layout = usePanelLayout();
  const areaRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });

  useLayoutEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    const measure = () => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      setSize((prev) => (prev.w === w && prev.h === h ? prev : { w, h }));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useMoreBelow(areaRef);

  const root = layout?.root ?? null;
  const mins = usePaneMins(size.w);
  const geometry = useMemo(
    () => (root ? layoutPanes(root, size.w, size.h, mins) : null),
    [root, size.w, size.h, mins],
  );
  // Type order, not tree order, so no edit ever reorders the DOM.
  const leaves = geometry
    ? [...geometry.leaves].sort(
        (a, b) => PANE_TYPES.indexOf(a.leaf.pane) - PANE_TYPES.indexOf(b.leaf.pane),
      )
    : [];

  return (
    <div className="panel-host">
      <div ref={areaRef} className="panel-panes">
        {leaves.map(({ leaf, rect }) => (
          <section
            key={leaf.pane}
            className={`pane pane-${leaf.pane}`}
            aria-label={PANE_LABELS[leaf.pane]}
            style={{
              left: rect.x,
              top: rect.y,
              width: rect.w,
              height: rect.h,
              overflowY: rect.h < PANE_MIN_H[leaf.pane] ? 'auto' : undefined,
            }}
          >
            <PaneLeafContext.Provider value={leaf}>{PANES[leaf.pane]()}</PaneLeafContext.Provider>
          </section>
        ))}
        {geometry?.handles.map((h) => (
          <PaneHandle key={`${h.parentId}:${h.index}`} handle={h} />
        ))}
        {root && root.children.length === 0 && (
          <p className="pane-empty panel-empty">
            Add a pane to show the map, your affects, your group, or chat.
          </p>
        )}
      </div>
      <VitalsFooter />
    </div>
  );
}

/** Mark every pane body that has rows below its bottom edge with
 *  `data-more="below"`, which panel.css fades so you can tell the list
 *  goes on. Scrolling to the end, a taller pane, or fewer rows clear
 *  it. Pane bodies come and go with the tree, so this watches the
 *  pane area for them. */
function useMoreBelow(areaRef: RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const area = areaRef.current;
    if (!area) return;
    const check = (body: HTMLElement) => {
      const more = body.scrollHeight - body.scrollTop - body.clientHeight > 1;
      if (more === (body.dataset.more === 'below')) return;
      if (more) body.dataset.more = 'below';
      else delete body.dataset.more;
    };
    // The body resizes with its pane, and its children with its rows.
    const watched = new WeakSet<Element>();
    const resize = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const body = entry.target.closest('.pane-body');
        if (body instanceof HTMLElement) check(body);
      }
    });
    const scan = () => {
      for (const body of area.querySelectorAll<HTMLElement>('.pane-body')) {
        for (const el of [body, ...Array.from(body.children)]) {
          if (watched.has(el)) continue;
          watched.add(el);
          resize.observe(el);
        }
        check(body);
      }
    };
    const mutations = new MutationObserver(scan);
    mutations.observe(area, { childList: true, subtree: true });
    // Scroll does not bubble, so listen on the way down.
    const onScroll = (e: Event) => {
      const t = e.target;
      if (t instanceof HTMLElement && t.classList.contains('pane-body')) check(t);
    };
    area.addEventListener('scroll', onScroll, true);
    scan();
    return () => {
      resize.disconnect();
      mutations.disconnect();
      area.removeEventListener('scroll', onScroll, true);
    };
  }, [areaRef]);
}

/** The line between two sibling panes. Drag it, or focus it and use
 *  the arrow keys. Neither neighbour goes below its minimum. */
function PaneHandle({ handle }: { handle: HandleBox }) {
  const drag = useRef<{ start: number; sizes: number[] } | null>(null);
  const vertical = handle.dir === 'column';
  const { rect, index, mins } = handle;
  const resize = (sizes: number[], delta: number) =>
    dragSizes(sizes, index, delta, mins[index], mins[index + 1]);

  const apply = (sizes: number[]) => {
    const root = getPanelLayout()?.root;
    if (!root) return;
    setPaneTree(
      setWeights(
        root,
        handle.parentId,
        sizes.map((s) => Math.max(1, s)),
      ),
    );
  };

  const coord = (e: PointerEvent) => (vertical ? e.clientY : e.clientX);
  // The body carries the resize cursor for the whole drag, so it holds
  // when the pointer runs ahead of the 1 px line.
  const endDrag = () => {
    if (!drag.current) return;
    drag.current = null;
    document.body.style.cursor = '';
  };

  return (
    <div
      role="separator"
      aria-orientation={vertical ? 'horizontal' : 'vertical'}
      aria-label={vertical ? 'Resize the panes above and below' : 'Resize the panes on each side'}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(
        (handle.sizes[handle.index] /
          Math.max(1, handle.sizes[handle.index] + handle.sizes[handle.index + 1])) *
          100,
      )}
      tabIndex={0}
      className={`pane-handle ${vertical ? 'pane-handle-h' : 'pane-handle-v'}`}
      style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { start: coord(e), sizes: handle.sizes };
        document.body.style.cursor = vertical ? 'row-resize' : 'col-resize';
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        apply(resize(d.sizes, coord(e) - d.start));
      }}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onKeyDown={(e) => {
        const step = e.shiftKey ? KEY_STEP_BIG : KEY_STEP;
        const back = vertical ? 'ArrowUp' : 'ArrowLeft';
        const fwd = vertical ? 'ArrowDown' : 'ArrowRight';
        if (e.key !== back && e.key !== fwd) return;
        e.preventDefault();
        apply(resize(handle.sizes, e.key === fwd ? step : -step));
      }}
    />
  );
}
