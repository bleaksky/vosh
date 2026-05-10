import { useEffect, useState, type ReactNode } from 'react';
import { MapPane } from './MapPane';
import { AffectsPane } from './AffectsPane';
import { InfoTabsPane } from './InfoTabsPane';
import {
  loadSidebarOrder,
  subscribeSidebarOrder,
  type SidebarPanelId,
} from '../lib/sidebarOrder';

const PANE_RENDERERS: Record<SidebarPanelId, () => ReactNode> = {
  'affects-pane': () => <AffectsPane />,
  'map-pane': () => <MapPane />,
  'info-tabs-pane': () => <InfoTabsPane />,
};

export function SidePanel() {
  const [order, setOrder] = useState<SidebarPanelId[]>(loadSidebarOrder);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    subscribeSidebarOrder((next) => {
      setOrder(next);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <aside className="side-panel" aria-label="side panel">
      {order.map((id) => {
        const render = PANE_RENDERERS[id];
        if (!render) return null;
        return (
          <div key={id} className="sidebar-section" data-panel-id={id}>
            {render()}
          </div>
        );
      })}
    </aside>
  );
}
