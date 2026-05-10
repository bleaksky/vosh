import { useEffect, useState } from 'react';
import {
  dockLayoutGet,
  dockLayoutSet,
  type DockEntryPersist,
} from '../lib/session';
import type { SnapZone } from '../lib/docking';
import {
  broadcastSidebarOrder,
  loadSidebarOrder,
  moveSidebarPanel,
  SIDEBAR_PANEL_IDS,
  type SidebarPanelId,
} from '../lib/sidebarOrder';

interface Props {
  onError: (message: string) => void;
}

// `sidebar` is the synthetic zone used by the editor to mean "leave
// this section in the default side panel." It's not a real SnapZone
// and gets filtered out before saving.
type EditorZone = SnapZone | 'sidebar';

const PANELS: { id: string; label: string }[] = [
  { id: 'affects-pane', label: 'Affects' },
  { id: 'map-pane', label: 'Map' },
  { id: 'info-tabs-pane', label: 'Chat / Wealth / Group' },
];

const ZONES: { id: EditorZone; label: string; row: number; col: number }[] = [
  { id: 'top-left', label: '⌜', row: 1, col: 1 },
  { id: 'top', label: 'Top', row: 1, col: 2 },
  { id: 'top-right', label: '⌝', row: 1, col: 3 },
  { id: 'left', label: 'Left', row: 2, col: 1 },
  { id: 'sidebar', label: 'Side panel', row: 2, col: 2 },
  { id: 'right', label: 'Right', row: 2, col: 3 },
  { id: 'bottom-left', label: '⌞', row: 3, col: 1 },
  { id: 'bottom', label: 'Bottom', row: 3, col: 2 },
  { id: 'bottom-right', label: '⌟', row: 3, col: 3 },
];

function entriesToMap(entries: DockEntryPersist[]): Record<string, EditorZone> {
  const map: Record<string, EditorZone> = {};
  for (const p of PANELS) map[p.id] = 'sidebar';
  for (const e of entries) {
    if (PANELS.some((p) => p.id === e.id) && ZONES.some((z) => z.id === e.zone)) {
      map[e.id] = e.zone as EditorZone;
    }
  }
  return map;
}

function isSidebarPanelId(id: string): id is SidebarPanelId {
  return (SIDEBAR_PANEL_IDS as readonly string[]).includes(id);
}

export function LayoutEditorPanel({ onError }: Props) {
  const [placement, setPlacement] = useState<Record<string, EditorZone>>(() =>
    entriesToMap([]),
  );
  const [savedPlacement, setSavedPlacement] = useState<Record<string, EditorZone>>(() =>
    entriesToMap([]),
  );
  const [sidebarOrder, setSidebarOrder] = useState<SidebarPanelId[]>(loadSidebarOrder);
  const [savedSidebarOrder, setSavedSidebarOrder] =
    useState<SidebarPanelId[]>(loadSidebarOrder);
  const [dragging, setDragging] = useState<string | null>(null);
  const [hoverZone, setHoverZone] = useState<EditorZone | null>(null);
  const [status, setStatus] = useState('');

  useEffect(() => {
    let cancelled = false;
    dockLayoutGet()
      .then((entries) => {
        if (cancelled) return;
        const map = entriesToMap(entries);
        setPlacement(map);
        setSavedPlacement(map);
      })
      .catch((e) => onError(`load dock layout failed: ${String(e)}`));
    return () => {
      cancelled = true;
    };
  }, [onError]);

  const placementDirty = PANELS.some(
    (p) => placement[p.id] !== savedPlacement[p.id],
  );
  const orderDirty =
    sidebarOrder.length !== savedSidebarOrder.length ||
    sidebarOrder.some((id, idx) => savedSidebarOrder[idx] !== id);
  const dirty = placementDirty || orderDirty;

  const place = (panelId: string, zone: EditorZone) => {
    setPlacement((prev) => ({ ...prev, [panelId]: zone }));
  };

  const handleDragStart = (panelId: string) => (e: React.DragEvent) => {
    e.dataTransfer.setData('text/plain', panelId);
    e.dataTransfer.effectAllowed = 'move';
    setDragging(panelId);
  };

  const handleDragEnd = () => {
    setDragging(null);
    setHoverZone(null);
  };

  const handleZoneDragOver = (zone: EditorZone) => (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (hoverZone !== zone) setHoverZone(zone);
  };

  const handleZoneDragLeave = () => {
    setHoverZone(null);
  };

  const handleZoneDrop = (zone: EditorZone) => (e: React.DragEvent) => {
    e.preventDefault();
    // Prefer React state over dataTransfer.getData. Tauri's WebKit
    // sometimes loses dataTransfer between the dragstart and drop
    // events depending on platform; the dragging state was set in
    // handleDragStart in the same window so it's always reliable.
    const panelId = dragging ?? e.dataTransfer.getData('text/plain');
    if (panelId) place(panelId, zone);
    setDragging(null);
    setHoverZone(null);
  };

  const moveSidebar = (id: SidebarPanelId, direction: 'up' | 'down') => {
    setSidebarOrder((prev) => moveSidebarPanel(prev, id, direction));
  };

  const handleSave = async () => {
    setStatus('saving…');
    try {
      const entries: DockEntryPersist[] = PANELS.filter(
        (p) => placement[p.id] !== 'sidebar',
      ).map((p) => ({ id: p.id, zone: placement[p.id] as SnapZone }));
      await dockLayoutSet(entries);
      await broadcastSidebarOrder(sidebarOrder);
      setSavedPlacement(placement);
      setSavedSidebarOrder(sidebarOrder);
      setStatus(
        `saved (${entries.length} docked, ${PANELS.length - entries.length} in sidebar)`,
      );
    } catch (e) {
      setStatus(`save failed: ${String(e)}`);
      onError(`save failed: ${String(e)}`);
    }
  };

  const handleRevert = () => {
    setPlacement(savedPlacement);
    setSidebarOrder(savedSidebarOrder);
    setStatus('reverted to last saved');
  };

  const handleAllSidebar = () => {
    const reset: Record<string, EditorZone> = {};
    for (const p of PANELS) reset[p.id] = 'sidebar';
    setPlacement(reset);
  };

  const panelsByZone: Record<EditorZone, string[]> = {} as Record<EditorZone, string[]>;
  for (const z of ZONES) panelsByZone[z.id] = [];
  for (const p of PANELS) {
    panelsByZone[placement[p.id]].push(p.id);
  }
  // Render the sidebar zone in the user's chosen order, not the
  // arbitrary insertion order. Anything in `panelsByZone.sidebar`
  // that's missing from the saved order falls back to the end.
  panelsByZone.sidebar = [
    ...sidebarOrder.filter((id) => panelsByZone.sidebar.includes(id)),
    ...panelsByZone.sidebar.filter((id) => !sidebarOrder.includes(id as SidebarPanelId)),
  ];

  return (
    <div className="layout-editor-embedded">
      <p className="layout-editor-hint">
        Drag a panel from the list onto a zone. Center cell is the sidebar
        (default position). Drop another panel on the same zone to stack;
        drop back on sidebar to undock. Save applies the layout to the main
        window without restarting.
      </p>
      <div className="layout-editor-actions">
        <button type="button" onClick={handleAllSidebar}>
          all to sidebar
        </button>
        <button type="button" onClick={handleRevert} disabled={!dirty}>
          revert
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={!dirty}
          className="layout-editor-save"
        >
          save and apply
        </button>
      </div>
      <div className="layout-editor-body">
        <div className="layout-editor-panels">
          <h3>Panels</h3>
          <ul>
            {PANELS.map((p) => {
              const zone = placement[p.id];
              const isDragging = dragging === p.id;
              return (
                <li key={p.id}>
                  <div
                    className={`layout-panel-card${isDragging ? ' is-dragging' : ''}`}
                    draggable
                    onDragStart={handleDragStart(p.id)}
                    onDragEnd={handleDragEnd}
                  >
                    <span className="layout-panel-name">{p.label}</span>
                    <span className="layout-panel-zone">{zone}</span>
                  </div>
                </li>
              );
            })}
          </ul>
          {dragging && (
            <p className="layout-editor-pickedhint">
              Dragging <strong>{PANELS.find((p) => p.id === dragging)?.label}</strong>.
              Drop on any zone.
            </p>
          )}
        </div>
        <div className="layout-editor-grid" aria-label="dock zones">
          {ZONES.map((zone) => {
            const occupants = panelsByZone[zone.id];
            const isHover = hoverZone === zone.id;
            return (
              <div
                key={zone.id}
                className={`layout-zone layout-zone-${zone.id}${
                  dragging ? ' is-target' : ''
                }${isHover ? ' is-hover' : ''}${
                  occupants.length > 0 ? ' is-occupied' : ''
                }`}
                style={{ gridRow: zone.row, gridColumn: zone.col }}
                onDragOver={handleZoneDragOver(zone.id)}
                onDragLeave={handleZoneDragLeave}
                onDrop={handleZoneDrop(zone.id)}
              >
                <span className="layout-zone-label">{zone.label}</span>
                {occupants.length > 0 && (
                  <ul className="layout-zone-occupants">
                    {occupants.map((id, idx) => {
                      const panel = PANELS.find((p) => p.id === id);
                      const isPanelDragging = dragging === id;
                      const showOrderControls =
                        zone.id === 'sidebar' &&
                        isSidebarPanelId(id) &&
                        occupants.length > 1;
                      return (
                        <li key={id}>
                          <div
                            className={`layout-zone-occupant${
                              isPanelDragging ? ' is-dragging' : ''
                            }`}
                            draggable
                            onDragStart={handleDragStart(id)}
                            onDragEnd={handleDragEnd}
                          >
                            <span className="layout-zone-occupant-label">{panel?.label}</span>
                            {showOrderControls && isSidebarPanelId(id) && (
                              <span className="layout-zone-occupant-controls">
                                <button
                                  type="button"
                                  aria-label="move up"
                                  title="move up"
                                  disabled={idx === 0}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    moveSidebar(id, 'up');
                                  }}
                                >
                                  ↑
                                </button>
                                <button
                                  type="button"
                                  aria-label="move down"
                                  title="move down"
                                  disabled={idx === occupants.length - 1}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    moveSidebar(id, 'down');
                                  }}
                                >
                                  ↓
                                </button>
                              </span>
                            )}
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
            );
          })}
        </div>
      </div>
      <div className="layout-editor-status">{status}</div>
    </div>
  );
}
