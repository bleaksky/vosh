# Settings

`src/SettingsApp.tsx` is the frame. It draws the sidebar (`Sidebar.tsx`), the breadcrumb band, and one page per group. A page built from its board lives in `pages/`, and a group still waiting for its board keeps its placeholder in `groups/`. Every page builds from the primitives in `ui/`.

## Pages

A page is a component in `pages/` or `groups/` that takes `SettingsPageProps` from `pageTypes.ts`.

- `target` is where the page should land. `target.section` and `target.anchor` come from the nav, a deep link, or a search hit.
- `navSeq` goes up on every navigation, even to the same target. React to it when the target changes state on the page, like the Automation kind or the Characters profile.
- `config`, `setConfig`, and `onError` are the window's UiConfig copy, its setter, and the error line above the page. Every save sends the whole snapshot, so never keep a second copy of the config.
- `pathB` is true in loadout mode.
- `navigate(target)` goes somewhere else in Settings.

Register a page in `PAGES` in `SettingsApp.tsx`. A page that pins its own bar and scrolls inside itself, like the Automation save bar, sets `selfScroll` there.

A page that saves as you go takes `update` from `useSettingsAutoSave` in `legacy/`. `update(patch)` patches the config copy and saves the whole snapshot once typing settles. Pass `{ now: true }` for a discrete pick another window shows at once, like a theme or a toggle.

A page built on its board lives in `pages/`. `pages/CharactersPage.tsx` is the Characters board, with its parts in `pages/characters/`. `pages/AppearancePage.tsx` is the Appearance board. Its parts sit in `pages/appearance/`. The split divider and sent command color rows are self contained, so either can move to another group by rendering it there with `config` and `update`.

The placeholder for Automation still shows the old editors under the board's headings. Replace the whole component with its board. General, Layout, and Input have no board yet. Their old editors sit in `legacy/` and render inside `LegacyIsland`, which marks them `data-interim`. The old Appearance and Characters placeholders, `groups/AppearanceGroup.tsx` and `groups/CharactersGroup.tsx`, no longer render.

## Deep links and search

A deep link is a string like `automation:macros` or `characters:Erelei#tracked`. `src/lib/settingsNav.ts` resolves it and maps every old tab id. `src/lib/settingsLink.ts` opens Settings on one from the main window.

What a section means depends on the group. In Automation it is the kind. In Characters it is the profile name, and no section means the active profile. Everywhere else it is a section `id` the frame scrolls to.

Search finds rows. `src/lib/settingsSearch.ts` lists every row with its label and target. When a page adds a row, add it there too, and give the element the same anchor, `anchor` on `Row` or `Disclosure` and `id` on `Section`. The frame scrolls to it and flashes a row (`revealAnchor.ts`). When the anchor sits inside a closed `Disclosure`, open it when `target.anchor` names it.

# Primitives

Import the primitives from `ui/index.ts`. A page in `groups/` does it like this.

```tsx
import { Section, Row, Toggle, Select } from '../ui';
```

The styles live in `src/styles/settings.css`. Every class starts with `st-` and reads only the One Window tokens (`--bg`, `--panel`, `--sep`, `--selrow`, `--inputband`, `--text`, `--secondary`, `--tertiary`, `--accent`, `--on-accent`, `--danger-text`, and the rest in `tokens.css`). The Settings root is not under `.settings-app`, so the legacy rules in `styles.css` never reach new markup. Keep it that way. Do not add a `settings-` class to new markup.

Use monospace only for MUD text. That means patterns, sent commands, macro keys, host, and port. Everything else uses the UI font with tabular numbers, which the root already sets.

## Layout blocks

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

`IconButton` is a 28×24 button that shows only a 16 px icon, the one the window controls use. It forwards its ref.

- `label` is its accessible name, like `Erelei options` or `Move Haste up`. It is required, since the button shows no text.
- `icon` is the icon.

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

Put the rows it opens in a `div` with the class `st-disclosure-panel` right after it in the same card. Each row in the panel draws the inset hairline, the first one included, and the last one takes the card's bottom corners.

`ColorField` is a color control for a row or a grid: a 16 px swatch that opens the system color picker, then the color as text, on the field fill.

- `value` is CSS color text, or an empty string for none. `onChange(value)` runs with each color the page can draw. Text that does not read as a color yet stays in the field, and leaving the field puts the saved color back.
- `allowEmpty` lets you clear the text, which runs `onChange('')`, for a color that falls back to the theme. `placeholder` names that fallback, like `Theme color`, and `emptySwatch` is the color the swatch shows meanwhile, var() included.
- `width` in px or any CSS length, 120 by default.
- `pickerLabel` names the swatch's picker, like `Choose the split divider color`. Inside a `Row` the row label names the text. Outside one, pass `id` for a `<label>` or `aria-label`.

`VisuallyHidden` holds text a screen reader reads and the page does not show, like a list row's On or Off.

`useRowIds` returns the ids of the enclosing `Row` for a custom control.

`cx` joins class names.

## Icons

`ui/icons.tsx` holds the SPEC 6 set. `GearIcon`, `AppearanceIcon`, `LayoutIcon`, `KeyboardIcon`, `BoltIcon`, `UserIcon`, `SearchIcon`, `ChevronRightIcon`, `ChevronDownIcon`, `ChevronUpIcon`, `CloseIcon`, `PlusIcon`, `CheckIcon`, `MoreIcon`, `MinimizeIcon`, and `MaximizeIcon`. Each takes `size` (16 by default, or 12) and `className`. A 12 px icon keeps the 1.25 px stroke.

## Focus and motion

Every control draws a 2 px accent outline 2 px out on keyboard focus. Transitions stop under reduced motion.
