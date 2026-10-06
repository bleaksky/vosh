import { useId, type ReactNode } from 'react';
import { ColorField } from '../../ui';

// Labeled color fields in a grid under a row: the base palette and a
// custom theme's slots.

export interface ColorSlot {
  key: string;
  label: string;
  value: string;
}

/** One labeled color field in the grid. */
function ColorCell({ slot, onChange }: { slot: ColorSlot; onChange: (value: string) => void }) {
  const id = useId();
  return (
    <div className="st-color-cell">
      <label htmlFor={id} className="st-color-cell-label">
        {slot.label}
      </label>
      <ColorField
        id={id}
        width="100%"
        value={slot.value}
        pickerLabel={`Choose the ${slot.label.toLowerCase()} color`}
        onChange={onChange}
      />
    </div>
  );
}

interface ColorGroupProps {
  /** A list heading over the group, or none for a lone grid. */
  heading?: ReactNode;
  slots: readonly ColorSlot[];
  onChange: (key: string, value: string) => void;
}

/** A grid of color fields, 128 px and up, 12 apart. */
export function ColorGroup({ heading, slots, onChange }: ColorGroupProps) {
  const headingId = useId();
  return (
    <div
      className="st-color-group"
      role="group"
      aria-labelledby={heading === undefined ? undefined : headingId}
    >
      {heading !== undefined && (
        <h3 id={headingId} className="st-color-heading">
          {heading}
        </h3>
      )}
      <div className="st-color-grid">
        {slots.map((slot) => (
          <ColorCell key={slot.key} slot={slot} onChange={(value) => onChange(slot.key, value)} />
        ))}
      </div>
    </div>
  );
}

/** The padded block that holds color groups inside a card. */
export function ColorBlock({ children }: { children: ReactNode }) {
  return <div className="st-color-block">{children}</div>;
}
