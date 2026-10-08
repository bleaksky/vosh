// The kit every window shares. Settings, Help and the prompt card build
// from these, so each piece keeps its geometry and its One Window tokens
// in one place. The controls draw from src/styles/controls.css, and the
// sections, cards and rows from src/styles/settings.css. README.md
// beside this file lists each one and its props.
//
// MenuSurface with menuPlacement, ConfirmDialog, CodeEditor and
// WindowControls are imported by path. CodeEditor would load CodeMirror
// into every file that imports this barrel, and WindowControls takes its
// icons from it.

export { Button, type ButtonProps, type ButtonVariant } from './Button';
export { Card, type CardProps } from './Card';
export { CardNote } from './CardNote';
export { clearCoach, menuRows, showCoach, type Coach } from './coach';
export { CoachRing } from './CoachRing';
export { Chip, ChipButton, type ChipButtonProps, type ChipProps } from './Chip';
export { ColorField, type ColorFieldProps } from './ColorField';
export { cx } from './cx';
export { Disclosure, DisclosurePanel, type DisclosureProps } from './Disclosure';
export { Field, type FieldProps } from './Field';
export { FieldArea, type FieldAreaProps } from './FieldArea';
export { IconButton, type IconButtonProps } from './IconButton';
export { Keycap } from './Keycap';
export { LinkRow, type LinkRowProps } from './LinkRow';
export { NumberField, type NumberFieldProps } from './NumberField';
export { Row, type RowProps } from './Row';
export { useRowIds, type RowIds } from './rowContext';
export { Section, type SectionHelp, type SectionProps } from './Section';
export { Segmented, type SegmentedOption, type SegmentedProps } from './Segmented';
export { Select, type SelectOption, type SelectProps } from './Select';
export { Toggle, type ToggleProps } from './Toggle';
export { VisuallyHidden } from './VisuallyHidden';
export * from './icons';
