import { type ReactNode } from 'react';
import { useChipStyle } from '../lib/useChipStyle';

interface ChipFrameProps {
  // Caption shown in `caption_value` style. Drop in lowercase short
  // form (e.g. "tick", "time").
  caption: string;
  // Unicode glyph shown in `icon_value` style. Keep to one cell-wide
  // characters that render across the bundled fonts.
  icon: string;
  // The chip's value node. Carries its own color so the warn / sky-
  // tint behavior lives on the value, not the frame.
  value: ReactNode;
  // Extra class names piped from the host (e.g. "statusbar-tick" so
  // host CSS can adjust spacing).
  className?: string;
  // Optional aria-label override for screen readers. Defaults to the
  // caption.
  ariaLabel?: string;
}

// Renders a single tick / mud-time / future chip in the user-picked
// style. Hosts (StatusBar, VitalsBar, RoomStrip, AffectsBar) drop a
// ChipFrame wherever they want the chip to live; the chip owns its
// appearance.
export function ChipFrame({ caption, icon, value, className, ariaLabel }: ChipFrameProps) {
  const style = useChipStyle();
  const cls = `chip-frame chip-frame-${style}${className ? ` ${className}` : ''}`;
  return (
    <span className={cls} aria-label={ariaLabel ?? caption}>
      {style === 'caption_value' && <span className="chip-frame-caption">{caption}</span>}
      {style === 'icon_value' && (
        <span className="chip-frame-icon" aria-hidden="true">
          {icon}
        </span>
      )}
      <span className="chip-frame-value">{value}</span>
    </span>
  );
}
