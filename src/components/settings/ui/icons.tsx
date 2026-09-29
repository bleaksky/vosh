import type { ReactNode } from 'react';

// The SPEC 6 icon set for Settings: 16 unit strokes at 1.25, round caps
// and joins, drawn in currentColor so each control sets the tone. At
// 12 px the stroke keeps its 1.25 px weight through vector-effect, the
// way the boards draw the chevrons and the chip close icon.

interface IconProps {
  /** Rendered size in px. 16 unless a recipe says 12. */
  size?: 12 | 16;
  className?: string;
}

function Glyph({ size = 16, className, children }: IconProps & { children: ReactNode }) {
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
      focusable="false"
      className={className}
    >
      {children}
    </svg>
  );
}

// A 12 px glyph keeps the 16 px stroke weight.
const scale = (size: IconProps['size']) =>
  size === 12 ? ({ vectorEffect: 'non-scaling-stroke' } as const) : {};

export function GearIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <circle cx="8" cy="8" r="2.25" />
      <path d="M8 1.75v1.75M8 12.5v1.75M1.75 8h1.75M12.5 8h1.75M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2" />
    </Glyph>
  );
}

export function AppearanceIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <circle cx="8" cy="8" r="6.25" />
      <path d="M8 1.75v12.5" />
      <path d="M8 1.75a6.25 6.25 0 0 1 0 12.5z" fill="currentColor" stroke="none" />
    </Glyph>
  );
}

export function LayoutIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M1.75 6.25h12.5M6.25 6.25v7" />
    </Glyph>
  );
}

export function KeyboardIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <rect x="1.75" y="4" width="12.5" height="8" rx="1.75" />
      <path d="M4.5 7h.01M7 7h.01M9.5 7h.01M12 7h.01M5 9.75h6" />
    </Glyph>
  );
}

export function BoltIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M9 1.75L3.75 9H8l-1 5.25L12.25 7H8z" />
    </Glyph>
  );
}

export function UserIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <circle cx="8" cy="5.5" r="2.75" />
      <path d="M2.75 14c.75-2.75 2.85-4.1 5.25-4.1s4.5 1.35 5.25 4.1" />
    </Glyph>
  );
}

export function SearchIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <circle cx="7" cy="7" r="4.5" {...scale(props.size)} />
      <path d="M10.5 10.5l3.5 3.5" {...scale(props.size)} />
    </Glyph>
  );
}

export function ChevronRightIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M6.25 4.5L9.75 8l-3.5 3.5" {...scale(props.size)} />
    </Glyph>
  );
}

export function ChevronDownIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M4.5 6.25L8 9.75l3.5-3.5" {...scale(props.size)} />
    </Glyph>
  );
}

export function ChevronUpIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M4.5 9.75L8 6.25l3.5 3.5" {...scale(props.size)} />
    </Glyph>
  );
}

export function CloseIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" {...scale(props.size)} />
    </Glyph>
  );
}

export function PlusIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M8 3.5v9M3.5 8h9" {...scale(props.size)} />
    </Glyph>
  );
}

export function CheckIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M3.5 8.5l3 3 6-7" {...scale(props.size)} />
    </Glyph>
  );
}

/** Two sheets, the front one whole: copy. */
export function CopyIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <rect x="5.25" y="5.25" width="8.5" height="8.5" rx="1.75" {...scale(props.size)} />
      <path
        d="M10.75 5.25v-1.5c0-.83-.67-1.5-1.5-1.5h-5.5c-.83 0-1.5.67-1.5 1.5v5.5c0 .83.67 1.5 1.5 1.5h1.5"
        {...scale(props.size)}
      />
    </Glyph>
  );
}

/** The more glyph is three filled dots with no stroke. */
export function MoreIcon({ size = 16, className }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="currentColor"
      stroke="none"
      aria-hidden="true"
      focusable="false"
      className={className}
    >
      <circle cx="3.5" cy="8" r="1.25" />
      <circle cx="8" cy="8" r="1.25" />
      <circle cx="12.5" cy="8" r="1.25" />
    </svg>
  );
}

// Window controls for the frameless Settings window on Windows and
// Linux, the same glyphs the main window's title band draws.

export function MinimizeIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <path d="M4 8h8" />
    </Glyph>
  );
}

export function MaximizeIcon(props: IconProps) {
  return (
    <Glyph {...props}>
      <rect x="4" y="4" width="8" height="8" rx="1" />
    </Glyph>
  );
}
