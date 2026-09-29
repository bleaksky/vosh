import type { ReactNode } from 'react';

// The One Window icon set (SPEC 6): 16 unit strokes at 1.25, round caps
// and joins, drawn in currentColor so each button sets the tone.

function Glyph({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function PlusIcon() {
  return (
    <Glyph>
      <path d="M8 3.5v9M3.5 8h9" />
    </Glyph>
  );
}

export function SearchIcon() {
  return (
    <Glyph>
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5l3.5 3.5" />
    </Glyph>
  );
}

export function PanelIcon() {
  return (
    <Glyph>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M10 2.75v10.5" />
    </Glyph>
  );
}

/** The 12 px chevron after the session title. The stroke keeps its
 *  1.25 px weight at the smaller size. */
export function ChevronDownIcon() {
  return (
    <Glyph size={12}>
      <path d="M4.5 6.25L8 9.75l3.5-3.5" vectorEffect="non-scaling-stroke" />
    </Glyph>
  );
}

// Window controls for the frameless window on Windows and Linux.

export function MinimizeIcon() {
  return (
    <Glyph>
      <path d="M4 8h8" />
    </Glyph>
  );
}

export function MaximizeIcon() {
  return (
    <Glyph>
      <rect x="4" y="4" width="8" height="8" rx="1" />
    </Glyph>
  );
}

export function CloseIcon() {
  return (
    <Glyph>
      <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
    </Glyph>
  );
}
