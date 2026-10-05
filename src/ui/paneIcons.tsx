// The icons the panes and the terminal's right click menu draw, from
// the One Window icon set (SPEC section 6): 16 px grid, 1.25 stroke in
// currentColor.

interface IconProps {
  size?: number;
  className?: string;
}

function Stroke({ size = 16, className, children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      className={className}
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

export function MoreIcon({ size = 16, className }: IconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="currentColor"
      stroke="none"
      aria-hidden="true"
    >
      <circle cx="3.5" cy="8" r="1.25" />
      <circle cx="8" cy="8" r="1.25" />
      <circle cx="12.5" cy="8" r="1.25" />
    </svg>
  );
}

export function ChevronRightIcon(props: IconProps) {
  return (
    <Stroke {...props}>
      <path d="M6.25 4.5L9.75 8l-3.5 3.5" />
    </Stroke>
  );
}

/** Drawn at 12 px beside small text, so the stroke keeps its width. */
export function ChevronDownIcon(props: IconProps) {
  return (
    <Stroke {...props}>
      <path d="M4.5 6.25L8 9.75l3.5-3.5" vectorEffect="non-scaling-stroke" />
    </Stroke>
  );
}

export function CheckIcon(props: IconProps) {
  return (
    <Stroke {...props}>
      <path d="M3.5 8.5l3 3 6-7" />
    </Stroke>
  );
}
