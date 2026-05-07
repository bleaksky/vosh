import { StatusPane } from './StatusPane';
import { ChatPane } from './ChatPane';

export function SidePanel() {
  return (
    <aside className="side-panel" aria-label="side panel">
      <StatusPane />
      <ChatPane />
    </aside>
  );
}
