# The shared kit

Every window builds from the primitives in `src/ui`. Settings, Help and the prompt card import them from `src/ui/index.ts`, and a Settings page in its folder does it like this.

```tsx
import { Section, Row, Toggle, Select } from '../../ui';
```

The control styles live in `src/styles/controls.css`, and the section, card and row styles in `src/styles/settings.css`. Every class starts with `st-` and reads only the shared tokens (`--bg`, `--panel`, `--sep`, `--selrow`, `--inputband`, `--text`, `--secondary`, `--tertiary`, `--accent`, `--on-accent`, `--danger-text`, and the rest in `tokens.css`). The Settings root is not under `.settings-app`, so the interim rules in `migration.css` never reach new markup. Keep it that way. Do not add a `settings-` class to new markup.

Use monospace only for MUD text. That means patterns, sent commands, macro keys, host, and port. Everything else uses the UI font with tabular numbers, which the root already sets.

## Layout blocks

`Section` holds a heading and a card.

- `title` is the h2, in sentence case.
- `id` is the deep link and search anchor. The frame scrolls to it.
- `actions` renders at the right end of the heading row, like the Appearance import hint and button.
- `card` wraps the children in a `Card`. It is true by default. Pass false to lay out your own cards or columns.

`Card` is the radius 12 block on the `--inputband` fill. It takes every div prop. `columns` sets its rows two by two with a 1 px line between the columns, like General's `Keep the same for every character`. Only rows below the first pair draw the hairline. Pass `card={false}` to the `Section` and put the `Card` in yourself.

`CardNote` is a quiet 11/15 line of copy at the head of a card, above its rows, like the note over a plugin's Manifest. `tone="warn"` sets it in the warn color after the pane status dot, for what needs you about the card's item, like a plugin Vosh stopped or a trigger that hides your prompt. `action` puts a button at the end of a warn note, for the way to fix it, like `Open notification settings` on an alert preset.

`Row` is one card row, 44 high at least, with padding 10 16.

- `label` labels the first `Toggle`, `Select`, or `Field` inside the row. You do not pass ids.
- `description` is the 11/15 secondary line under the label. The control is described by it.
- `descriptionTone` set to `danger` sets the description in the danger tone, for a line that says why the control's value is refused, like a profile name you have.
- `anchor` is the search and deep link anchor. The frame scrolls the row into view and flashes it.
- `children` is the control, right aligned.

Rows after the first in a card draw the inset hairline themselves. A row that holds two controls gives the second one its own `id` or `aria-label`, since both would otherwise take the row id.

## Controls

`Toggle` is a checkbox with role switch on the 38×22 track. Props are `checked` and `onChange(checked)`, plus any input prop. Outside a `Row`, pass `aria-label`.

`Segmented` is the segmented control.

- `options` is a list of `{ value, label, disabled?, name? }`. Give `name` when the label is a picture, like the Input caret shapes. It becomes the segment's accessible name and its tooltip.
- `value` is the pressed value, or null for none.
- `onChange(value)` runs on press.
- `label` names the group when it does not sit in a `Row`, like the Automation `Kind`.

`Select` is a native select drawn as a field, with the 12 px chevron.

- `value`, `onChange(value)`, and `options` as a list of `{ value, label, disabled? }`.
- `width` in px or any CSS length, 160 by default. The boards use 160 and 240.

`Field` is a text field. It forwards its ref.

- `value` and `onChange(value)`.
- `width` in px or any CSS length, 240 by default.
- `mono` sets MUD text in the terminal font.
- `icon` adds a leading 16 px icon, like the search icon on the Automation filter.
- `invalid` draws a danger ring inside the fill and sets `aria-invalid`, for text Vosh will not take.

`FieldArea` is a `Field` for text where a newline means something, like the commands a trigger or timer sends. At one line it looks exactly like `Field`, and it grows a line at a time. It takes `value`, `onChange`, `width`, and `mono` like `Field` and forwards its ref. A plain `Field` drops newlines, so use this one for any value that can hold them.

`ColorField` is a color setting in a row or a grid, like `Sent command color` on Input, `Divider color` on Layout, and the custom theme colors on Appearance. A 16 px swatch at the left opens the system color picker, and the color reads as text in the UI font on the field fill. The hex rules live in `src/ui/colorText.ts`.

- `value` is CSS color text, or an empty string for none. `onChange(value)` runs with each color the field reads. A hex saves as lowercase `#rrggbb` once it has six digits, or three when you press Enter or leave the field. A hex with alpha or any other CSS color saves as typed once the page can draw it. Text that does not read as a color yet stays in the field. Leaving the field puts the saved color back, and Escape does the same while you stay in it.
- `allowEmpty` lets you clear the text, which runs `onChange('')`, for a color that falls back to the theme. `placeholder` names that fallback, like `Theme default`, and `emptySwatch` is the color the swatch shows meanwhile, var() included. The picker then opens on that color.
- `width` in px or any CSS length, 160 by default.
- `pickerLabel` names the swatch's picker, like `Choose the divider color`. Inside a `Row` the row label names the text. Outside one, pass `id` for a `<label>` or `aria-label`.

`NumberField` is a whole number setting with its unit inside the field, like `Wait between pasted lines` in ms and the panel `Width` in pt. What you type stays a draft until Enter or leaving the field saves it, clamped to the bounds. Up and Down step it, and Escape puts the saved value back.

- `value`, `onChange(value)`, `min`, and `max`.
- `unit` is the short unit the field shows, and `unitName` is how a screen reader says it, like `milliseconds`.
- `width` is 88 by default. `step` is 1, and Shift steps ten times as far.

`Button` forwards its ref.

- `variant` is `secondary` (the default, a hairline ring), `primary` (accent fill, `--on-accent` text), or `danger` (danger text, no fill).
- `icon` adds a leading 16 px icon in the secondary color, like `New profile`.

`IconButton` is a 28×24 button that shows only a 16 px icon, the one the window controls use. It forwards its ref.

- `label` is its accessible name, like `Ilsabet options` or `Move Haste up`. It is required, since the button shows no text.
- `icon` is the icon.

`Keycap` draws one key. Build the keys with `shortcutKeys` from `src/lib/shortcuts.ts` so macOS reads ⌘ and the other systems read Ctrl.

`Chip` is a pill with an optional close button.

- `onRemove` draws the close button and runs on press.
- `removeLabel` names the close button, like `Stop tracking Haste`.
- `as` is `li` when the chips sit in a list.

`ChipButton` is a chip shaped button for the end of a chip row, like `Add affect…`. `icon` takes a 12 px icon.

`Disclosure` is a row sized button that opens more settings, like `Advanced`.

- `label` and `description` as on `Row`.
- `expanded` sets aria-expanded and turns the chevron down. Render the content after it and point `aria-controls` at it.
- `anchor` as on `Row`.

`DisclosurePanel` holds the rows a `Disclosure` opens, right after it in the same `Card`. Give it the id the Disclosure's `aria-controls` names. Every row or block inside draws the inset hairline, the first one included, and the last one takes the card's bottom corners. The `Advanced` cards on Appearance and Input, and General's `Advanced` on Windows and Linux, use it.

A row whose content sits under its label line at full width, like the prompt template on Input, is a `div` with the `st-block` class holding a `Row` for the label line and the content after it. The `Row` drops its own padding there, and the block draws the hairline like any row.

`LinkRow` is a row that goes somewhere else in Settings, like Layout's `Panes and tracked affects`, which opens Characters. It draws like a `Disclosure` without the open state.

- `label`, `description`, and `anchor` as on `Row`.
- `onClick` runs on press. Call the page's `navigate` there.

`ConfirmDialog` is the 320 wide card that asks before a choice, imported by path. Pass `title`, `body`, `confirmLabel`, `onConfirm` and `onCancel`. `tone` is `danger` by default and `primary` for a choice that makes something. `cancelLabel` names the other button, `Cancel` by default, like the banner ask's `Not now`.

`CoachRing` is Show me's ring, mounted once in each window. `showCoach({ find, line })` rings what `find` returns once it draws, 2 px out in the accent with one pulse, moves focus to the first of them and sets `line` beside it on the toast recipe. The pick, Esc, a press anywhere or a target that leaves the page clears it. `menuRows(menu, labels)` finds rows of an open menu by their labels. A Settings anchor with `data-st-coach` rings the same way when a deep link reaches it.

`VisuallyHidden` holds text a screen reader reads and the page does not show, like a list row's On or Off.

`useRowIds` returns the ids of the enclosing `Row` for a custom control.

`cx` joins class names.

A few classes in `settings.css` and `controls.css` cover small shapes that are not worth a component.

- `st-field-pair` sets two fields in one row 8 px apart, like General's host and port. Give the second field its own `id` and `aria-label`.
- `st-glyph-toggle` is a 28×24 on and off button drawn as text, like the find bar's `Aa`. It carries `aria-pressed`. Add `st-glyph-case` for the Aa weight.
- `st-meta` is quiet 11/15 text beside a heading. `data-tone="danger"` turns it to the danger color for an error, like a failed update check.

## Icons

`src/ui/icons.tsx` holds the icon set every window draws from. `GearIcon`, `ToothedGearIcon`, `AppearanceIcon`, `LayoutIcon`, `KeyboardIcon`, `BoltIcon`, `CodeIcon`, `UserIcon`, `SearchIcon`, `ChevronRightIcon`, `ChevronDownIcon`, `ChevronUpIcon`, `CloseIcon`, `PlusIcon`, `CheckIcon`, `CopyIcon`, `PlayIcon`, `MoreIcon`, `GripIcon`, `MinimizeIcon`, and `MaximizeIcon`, with `PlugIcon`, `TerminalIcon`, `TickIcon`, `LifebuoyIcon`, and `BookIcon` for the Help sections. Each takes `size` (16 by default, or 12) and `className`. A 12 px icon keeps the 1.25 px stroke.

`GearIcon` is the spoked gear beside General in Settings. `ToothedGearIcon` is the six tooth gear on the title band's Settings button, since the spoked one reads as a sun at that spot.

`Glyph` is the svg every stroked icon draws on. `src/shell/icons.tsx` draws the three glyphs only the main window needs on it, the panel toggle and the status line's tick ring and sun path, so they match the kit.

## Focus and motion

Every control draws a 2 px accent outline 2 px out on keyboard focus. Transitions stop under reduced motion.
