import { useEffect, useRef } from 'react';
import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
  type IDockviewPanelProps,
} from 'dockview';
import 'dockview-core/dist/styles/dockview.css';

import { Terminal, type TerminalHandle } from './Terminal';
import { StatusPane } from './StatusPane';
import { AffectsPane } from './AffectsPane';
import { MapPane } from './MapPane';
import { ChatPane } from './ChatPane';
import { GoldPane, CabalPane } from './WorthPanes';

export interface DockableMiddleProps {
  fontFamily: string;
  fontSize: number;
  onTerminalReady: (handle: TerminalHandle) => void;
  /** Saved layout JSON from profile.toml. Empty string means use default. */
  savedLayout: string;
  /** Called whenever the user mutates the layout. Debounced upstream. */
  onLayoutChange: (json: string) => void;
}

interface TerminalParams {
  fontFamily: string;
  fontSize: number;
  onReady: (handle: TerminalHandle) => void;
}

function TerminalPanel(props: IDockviewPanelProps<TerminalParams>) {
  return (
    <Terminal
      fontFamily={props.params.fontFamily}
      fontSize={props.params.fontSize}
      onReady={props.params.onReady}
    />
  );
}

function StatusPanel() {
  return <StatusPane />;
}

function AffectsPanel() {
  return <AffectsPane />;
}

function MapPanel() {
  return <MapPane />;
}

function ChatPanel() {
  return <ChatPane />;
}

function GoldPanel() {
  return <GoldPane />;
}

function CabalPanel() {
  return <CabalPane />;
}

const components = {
  terminal: TerminalPanel,
  status: StatusPanel,
  affects: AffectsPanel,
  map: MapPanel,
  chat: ChatPanel,
  gold: GoldPanel,
  cabal: CabalPanel,
};

/// Walk a parsed dockview layout and verify every panel's component
/// name is in our known components map. A layout serialized by an
/// older build can reference a component (e.g. 'info') that no longer
/// exists; restoring it leaves an empty phantom panel that obscures
/// the rest of the layout.
function layoutMatchesComponents(
  layout: unknown,
  known: Record<string, unknown>,
): boolean {
  if (!layout || typeof layout !== 'object') return false;
  const panels = (layout as { panels?: Record<string, { contentComponent?: string }> }).panels;
  if (!panels || typeof panels !== 'object') return false;
  for (const panel of Object.values(panels)) {
    if (!panel?.contentComponent || !(panel.contentComponent in known)) {
      return false;
    }
  }
  return true;
}

function buildDefault(api: DockviewApi, params: TerminalParams) {
  // Drop everything that may exist from a prior layout, then build the
  // default arrangement: terminal in the main area, side panes stacked
  // on the right. Chat/Gold/Cabal share one group at the bottom and
  // appear as native dockview tabs (no in-pane tab strip).
  api.clear();
  api.addPanel({
    id: 'terminal',
    component: 'terminal',
    title: 'terminal',
    params,
  });
  api.addPanel({
    id: 'status',
    component: 'status',
    title: 'status',
    position: { referencePanel: 'terminal', direction: 'right' },
  });
  api.addPanel({
    id: 'affects',
    component: 'affects',
    title: 'affects',
    position: { referencePanel: 'status', direction: 'below' },
  });
  api.addPanel({
    id: 'map',
    component: 'map',
    title: 'map',
    position: { referencePanel: 'affects', direction: 'below' },
  });
  api.addPanel({
    id: 'chat',
    component: 'chat',
    title: 'chat',
    position: { referencePanel: 'map', direction: 'below' },
  });
  // Gold and cabal land in the SAME group as chat by referencing the
  // chat panel directly — `position: { referencePanel, direction }`
  // omitted means "same group as the reference, as a sibling tab."
  api.addPanel({
    id: 'gold',
    component: 'gold',
    title: 'gold',
    position: { referencePanel: 'chat' },
  });
  api.addPanel({
    id: 'cabal',
    component: 'cabal',
    title: 'cabal',
    position: { referencePanel: 'chat' },
  });
  api.getPanel('chat')?.api.setActive();
}

export function DockableMiddle({
  fontFamily,
  fontSize,
  onTerminalReady,
  savedLayout,
  onLayoutChange,
}: DockableMiddleProps) {
  // Hold the live values in refs so the dockview ready callback (run
  // once) can read them when restoring layout, and so the params
  // update path can reach the running panel.
  const apiRef = useRef<DockviewApi | null>(null);
  const paramsRef = useRef<TerminalParams>({
    fontFamily,
    fontSize,
    onReady: onTerminalReady,
  });
  paramsRef.current = { fontFamily, fontSize, onReady: onTerminalReady };

  const onReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    apiRef.current = api;

    // Restore from saved layout if present, parseable, and compatible
    // with the current panel set. Layouts saved before chat/gold/cabal
    // were split out of InfoTabsPane reference an `info` component
    // that no longer exists; restoring them leaves a phantom panel
    // that hides the terminal. Validate first.
    let restored = false;
    if (savedLayout) {
      try {
        const parsed = JSON.parse(savedLayout);
        if (layoutMatchesComponents(parsed, components)) {
          api.fromJSON(parsed);
          const term = api.getPanel('terminal');
          if (term) {
            term.api.updateParameters(paramsRef.current);
            restored = true;
          }
        }
      } catch {
        restored = false;
      }
    }
    if (!restored) {
      buildDefault(api, paramsRef.current);
    }

    // Persist on every structural mutation. Debounced via the upstream
    // callback so we don't spam the backend during a drag.
    const save = () => {
      try {
        onLayoutChange(JSON.stringify(api.toJSON()));
      } catch {
        // serialization can fail mid-drag; ignore
      }
    };
    const subs = [
      api.onDidAddPanel(save),
      api.onDidRemovePanel(save),
      api.onDidMovePanel(save),
      api.onDidAddGroup(save),
      api.onDidRemoveGroup(save),
      api.onDidLayoutFromJSON(save),
    ];
    return () => subs.forEach((d) => d.dispose());
  };

  // Reset to default layout when the user clicks "panes" (or any
  // other path that fires the reset event). Useful for recovering
  // from a layout that's hidden everything.
  useEffect(() => {
    const handler = () => {
      const api = apiRef.current;
      if (!api) return;
      buildDefault(api, paramsRef.current);
    };
    window.addEventListener('mudclient:layout-reset', handler);
    return () => window.removeEventListener('mudclient:layout-reset', handler);
  }, []);

  // When the font changes, push it into the running terminal panel via
  // updateParameters so the panel's React props refresh.
  useEffect(() => {
    const api = apiRef.current;
    if (!api) return;
    const term = api.getPanel('terminal');
    if (!term) return;
    term.api.updateParameters({ fontFamily, fontSize });
  }, [fontFamily, fontSize]);

  return (
    <DockviewReact
      components={components}
      onReady={onReady}
      // Use dockview's default tab component (it carries the correct
      // drag/drop wiring); the close X is hidden via CSS. singleTabMode
      // 'fullwidth' makes a group with one panel render its tab as a
      // full-width header bar, matching the old .pane-header look.
      singleTabMode="fullwidth"
      className="dockable-middle dockview-theme-abyss"
    />
  );
}
