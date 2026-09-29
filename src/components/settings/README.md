# Settings primitives

Every Settings page builds from the primitives in `ui/`. Import them from `ui/index.ts`.

```tsx
import { Section, Row, Toggle, Select } from './ui';
```

The styles live in `src/styles/settings.css`. Every class starts with `st-` and reads only the One Window tokens (`--bg`, `--panel`, `--sep`, `--selrow`, `--inputband`, `--text`, `--secondary`, `--tertiary`, `--accent`, `--on-accent`, `--danger-text`, and the rest in `tokens.css`). The Settings root is not under `.settings-app`, so the legacy rules in `styles.css` never reach new markup. Keep it that way. Do not add a `settings-` class to new markup.

Use monospace only for MUD text. That means patterns, sent commands, macro keys, host, and port. Everything else uses the UI font with tabular numbers, which the root already sets.

## Layout

`Section` holds a heading and a card.

- `title` is the h2, in sentence case.
- `id` is the deep link and search anchor. The frame scrolls to it.
- `actions` renders at the right end of the heading row, like the Appearance import hint and button.
- `card` wraps the children in a `Card`. It is true by default. Pass false to lay out your own cards or columns.

`Card` is the radius 12 block on the `--inputband` fill. It takes every div prop. `padded` adds 16 px of padding for a card that holds a block instead of rows.

`Row` is one card row, 44 high at least, with padding 10 16.

- `label` labels the first `Toggle`, `Select`, or `Field` inside the row. You do not pass ids.
- `description` is the 11/15 secondary line under the label. The control is described by it.
- `anchor` is the search and deep link anchor. The frame scrolls the row into view and flashes it.
- `children` is the control, right aligned.

Rows after the first in a card draw the inset hairline themselves. A row that holds two controls gives the second one its own `id` or `aria-label`, since both would otherwise take the row id.

## Controls

`Toggle` is a checkbox with role switch on the 38×22 track. Props are `checked` and `onChange(checked)`, plus any input prop. Outside a `Row`, pass `aria-label`.

`Segmented` is the segmented control.

- `options` is a list of `{ value, label, disabled? }`.
- `value` is the pressed value, or null for none.
- `onChange(value)` runs on press.
- `label` names the group when it does not sit in a `Row`, like the Automation `Kind`.

`Select` is a native select drawn as a field, with the 12 px chevron.

- `value`, `onChange(value)`, and `options` as a list of `{ value, label, disabled? }`.
- `width` in px, 160 by default. The boards use 160 and 240.

`Field` is a text field. It forwards its ref.

- `value` and `onChange(value)`.
- `width` in px or any CSS length, 240 by default.
- `mono` sets MUD text in the terminal font.
- `icon` adds a leading 16 px icon, like the search icon on the Automation filter.

`Button` forwards its ref.

- `variant` is `secondary` (the default, a hairline ring), `primary` (accent fill, `--on-accent` text), or `danger` (danger text, no fill).
- `icon` adds a leading 16 px icon in the secondary color, like `New profile`.

`Keycap` draws one key. Build the keys with `shortcutKeys` from `src/lib/palette.ts` so macOS reads ⌘ and the other systems read Ctrl.

`Chip` is a pill with an optional close button.

- `onRemove` draws the close button and runs on press.
- `removeLabel` names the close button, like `Stop tracking Haste`.
- `as` is `li` when the chips sit in a list.

`ChipButton` is a chip shaped button for the end of a chip row, like `Add affect…`. `icon` takes a 12 px icon.

`Disclosure` is a row sized button that opens more settings, like `Advanced`.

- `label` and `description` as on `Row`.
- `expanded` sets aria-expanded and turns the chevron down. Render the content after it and point `aria-controls` at it.
- `anchor` as on `Row`.

`VisuallyHidden` holds text a screen reader reads and the page does not show, like a list row's On or Off.

`useRowIds` returns the ids of the enclosing `Row` for a custom control.

`cx` joins class names.

## Icons

`ui/icons.tsx` holds the SPEC 6 set. `GearIcon`, `AppearanceIcon`, `LayoutIcon`, `KeyboardIcon`, `BoltIcon`, `UserIcon`, `SearchIcon`, `ChevronRightIcon`, `ChevronDownIcon`, `CloseIcon`, `PlusIcon`, `CheckIcon`, `MoreIcon`, `MinimizeIcon`, and `MaximizeIcon`. Each takes `size` (16 by default, or 12) and `className`. A 12 px icon keeps the 1.25 px stroke.

## Focus and motion

Every control draws a 2 px accent outline 2 px out on keyboard focus. Transitions stop under reduced motion.
