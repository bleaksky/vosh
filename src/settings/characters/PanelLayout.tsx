import { useMemo } from 'react';
import type { PaneLayout } from '../../panel/paneLayout';
import { paneSchematic } from './paneSchematic';
import { paneLabel } from '../../panel/paneTypes';
import { Button, Card, Section } from '../../ui';

// Panel layout on the Characters page: the schematic of the selected
// profile's panel, drawn from its real pane tree, beside a line about
// what Vosh saves and Reset to default.

interface Props {
  /** The name the schematic's sentence uses, like `Ilsabet`. */
  owner: string;
  panes: PaneLayout;
  onReset: () => void;
  resetting: boolean;
}

export function PanelLayout({ owner, panes, onReset, resetting }: Props) {
  const drawing = useMemo(() => paneSchematic(panes.root, paneLabel, owner), [panes.root, owner]);
  return (
    <Section id="layout" title="Panel layout" card={false}>
      <Card className="st-layout">
        <svg
          className="st-schematic"
          width={drawing.width}
          height={drawing.height}
          viewBox={`0 0 ${drawing.width} ${drawing.height}`}
          role="img"
          aria-label={drawing.ariaLabel}
        >
          <rect
            className="st-schematic-frame"
            x="0.5"
            y="0.5"
            width={drawing.width - 1}
            height={drawing.height - 1}
            rx="7.5"
          />
          <path className="st-schematic-lines" d={drawing.lines} />
          {drawing.labels.map((label) => (
            <text
              key={`${label.text} ${label.x} ${label.y}`}
              className="st-schematic-label"
              x={label.x}
              y={label.y}
            >
              {label.text}
            </text>
          ))}
        </svg>
        <div className="st-layout-side">
          <p className="st-layout-note">Vosh saves the panes you arrange for each character.</p>
          <Button onClick={onReset} disabled={resetting}>
            Reset to default
          </Button>
        </div>
      </Card>
    </Section>
  );
}
