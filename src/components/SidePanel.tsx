import { StatusPane } from './StatusPane';
import { MapPane } from './MapPane';
import { ChatPane } from './ChatPane';

export function SidePanel() {
  return (
    <aside className="side-panel" aria-label="side panel">
      <StatusPane />
      <MapPane />
      <ChatPane />
    </aside>
  );
}
