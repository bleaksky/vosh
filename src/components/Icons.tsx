// Stroke-SVG glyphs shared by the settings editors. The settings redo
// bans text dingbats (▸ ▾ ×) in favor of stroked paths at the mockup
// metrics: 12px chevrons for collapse toggles, a 9px cross for remove.

export function Chevron({ open, up = false }: { open: boolean; up?: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {up ? (
        <path d="M3 7.5 6 4.5 9 7.5" />
      ) : open ? (
        <path d="M3 4.5 6 7.5 9 4.5" />
      ) : (
        <path d="M4.5 3 7.5 6 4.5 9" />
      )}
    </svg>
  );
}

export function XIcon() {
  return (
    <svg
      width="9"
      height="9"
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />
    </svg>
  );
}

// Crossed swords for the combat target row in the vitals card.
export function Swords() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M2.5 2.5 11 11M13.5 2.5 5 11M9.5 12.5l3-3M3.5 9.5l3 3M11 11l2.5 2.5M5 11l-2.5 2.5" />
    </svg>
  );
}
