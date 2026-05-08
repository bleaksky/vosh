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
import { InfoTabsPane } from './InfoTabsPane';

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

function InfoPanel() {
  return <InfoTabsPane />;
}

const components = {
  terminal: TerminalPanel,
  status: StatusPanel,
  affects: AffectsPanel,
  map: MapPanel,
  info: InfoPanel,
};

function buildDefault(api: DockviewApi, params: TerminalParams) {
  // Drop everything that may exist from a prior layout, then build the
  // default arrangement: terminal in the main area, side panes stacked
  // on the right.
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
    id: 'info',
    component: 'info',
    title: 'chat / gold / cabal',
    position: { referencePanel: 'map', direction: 'below' },
  });
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

    // Restore from saved layout if present and parseable. Restored
    // layouts don't carry our terminal params (they're closures), so
    // re-set them after restoring.
    let restored = false;
    if (savedLayout) {
      try {
        api.fromJSON(JSON.parse(savedLayout));
        const term = api.getPanel('terminal');
        if (term) {
          term.api.updateParameters(paramsRef.current);
          restored = true;
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
      className="dockable-middle"
    />
  );
}
