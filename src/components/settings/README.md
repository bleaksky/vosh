# Settings

`src/SettingsApp.tsx` is the frame. It draws the sidebar (`Sidebar.tsx`), the breadcrumb band, and one page per group. A page built from its board lives in `pages/`, and a group still waiting for its board keeps its placeholder in `groups/`. Every page builds from the primitives in `ui/`.

## Pages

A page is a component in `pages/` or `groups/` that takes `SettingsPageProps` from `pageTypes.ts`.

- `target` is where the page should land. `target.section` and `target.anchor` come from the nav, a deep link, or a search hit.
- `navSeq` goes up on every navigation, even to the same target. React to it when the target changes state on the page, like the Automation kind or the Characters profile.
- `config`, `setConfig`, and `onError` are the window's UiConfig copy, its setter, and the error line above the page. Every save sends the whole snapshot, so never keep a second copy of the config.
- `pathB` is true in loadout mode.
- `navigate(target)` goes somewhere else in Settings.
- `setLeaveGuard(guard)` registers a question the frame asks before it moves to another group. The guard gets a `proceed` callback and returns true to hold the move, then calls `proceed` once you confirm. Automation uses it to ask before it drops unsaved changes. Clear it with null when the page unmounts.

Register a page in `PAGES` in `SettingsApp.tsx`. A page that pins its own bar and scrolls inside itself, like the Automation save bar, sets `selfScroll` there. `selfScroll` can also be a function of the target, for a group where only some targets scroll on their own.

A group can hold a page inside it, like the session logs at `general:logs`. Name it in `SETTINGS_SUBPAGES` in `src/lib/settingsNav.ts` with its title. The breadcrumb then reads `Settings › General › Session logs` with the group as a link back, the nav keeps the group active, and the frame does not scroll to the section. The group's page draws the inner page when `settingsSubpage(target)` names it.

A page that saves as you go takes `update` from `useSettingsAutoSave` in `legacy/`. `update(patch)` patches the config copy and saves the whole snapshot once typing settles. Pass `{ now: true }` for a discrete pick another window shows at once, like a theme or a toggle.

A page built on its board lives in `pages/`. `pages/CharactersPage.tsx` is the Characters board, with its parts in `pages/characters/`. `pages/AppearancePage.tsx` is the Appearance board. Its parts sit in `pages/appearance/`. The split divider color lives only on Layout and the sent command color only on Input, so Appearance's Advanced does not show them. `pages/AutomationPage.tsx` is the Automation board, described under Automation below.

`groups/GeneralGroup.tsx` is the General board, with the session log view in `groups/SessionLogs.tsx`. `groups/InputGroup.tsx` and `groups/LayoutGroup.tsx` are the Input and Layout boards. Layout's Status line section holds `rows/TickTimeStyleRow.tsx`, the Tick and time row, and under it `rows/TickCountRow.tsx`, the Tick counts row. The old Appearance, Automation, and Characters placeholders, `groups/AppearanceGroup.tsx`, `groups/AutomationGroup.tsx`, and `groups/CharactersGroup.tsx`, no longer render, and they were the last pages to show old editors from `legacy/` inside `LegacyIsland`.

## Deep links and search

A deep link is a string like `automation:macros` or `characters:Erelei#tracked`. `src/lib/settingsNav.ts` resolves it and maps every old tab id. `src/lib/settingsLink.ts` opens Settings on one from the main window.

What a section means depends on the group. In Automation it is the kind. In Characters it is the profile name, and no section means the active profile. Everywhere else it is a section `id` the frame scrolls to.

Search finds rows. `src/lib/settingsSearch.ts` lists every row with its label and target. When a page adds a row, add it there too, and give the element the same anchor, `anchor` on `Row` or `Disclosure` and `id` on `Section`. The frame scrolls to it and flashes a row (`revealAnchor.ts`). When the anchor sits inside a closed `Disclosure`, open it when `target.anchor` names it.

## Automation

`pages/AutomationPage.tsx` holds the kind switcher, `Import…`, and the discard question. Each kind is an editor in `pages/automation/` built on `DraftEditor`, which draws the list, the detail card, and the save bar over a draft from `src/lib/automationDraft.ts`. A kind is a `KindSpec` (`pages/automation/types.ts`): how it loads, saves through the kind's existing API, validates, lists, and draws its detail card. Timers pins the Tick above its list with a draft of its own. The pure logic, with tests, sits in `src/lib/automationDraft.ts`, `automationList.ts`, `automationTriggers.ts`, and `automationRecords.ts`.

Save writes only what the draft added, changed, and removed. Triggers and aliases go through one call that replaces the whole store, so their save reads the store again first and applies the draft over it by name (`saveTriggerDraft`, `saveAliasDraft`, `mergeDraftChanges`). Macros and timers save item by item, and a timer id always comes from the load. The backend sends `vosh://triggers-changed` and `vosh://aliases-changed` whenever a list changes, from Settings, #trigger, #alias, Lua, an import, or a preset (`src-tauri/src/list_events.rs`). A clean draft follows the store at once. A dirty one keeps your edits, says the list changed, and catches up once you save or discard.

The page draws the anchors `tick` (the Tick row), `import` (the import section), and `json` (`Edit all as JSON…` and the JSON view). A target with the `import` or `json` anchor opens that view, and `tick` selects the Tick.

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

`Card` is the radius 12 block on the `--inputband` fill. It takes every div prop. `padded` adds 16 px of padding for a card that holds a block instead of rows. `columns` sets its rows two by two with a 1 px line between the columns, like General's `Keep the same for every character`. Only rows below the first pair draw the hairline. Pass `card={false}` to the `Section` and put the `Card` in yourself.

`Row` is one card row, 44 high at least, with padding 10 16.

- `label` labels the first `Toggle`, `Select`, or `Field` inside the row. You do not pass ids.
- `description` is the 11/15 secondary line under the label. The control is described by it.
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

`FieldArea` is a `Field` for text where a newline means something, like the commands a trigger or timer sends. At one line it looks exactly like `Field`, and it grows a line at a time. It takes `value`, `onChange`, `width`, and `mono` like `Field` and forwards its ref. A plain `Field` drops newlines, so use this one for any value that can hold them.

`ColorField` is a color setting in a row or a grid, like `Sent command color` on Input, `Divider color` on Layout, and the custom theme colors on Appearance. A 16 px swatch at the left opens the system color picker, and the color reads as text in the UI font on the field fill. The hex rules live in `src/lib/colorField.ts`.

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

`DisclosurePanel` holds the rows a `Disclosure` opens, right after it in the same `Card`. Give it the id the Disclosure's `aria-controls` names. Every row or block inside draws the inset hairline, the first one included, and the last one takes the card's bottom corners. The `Advanced` cards on Appearance and Input, and General's `Advanced` on Windows and Linux, use it.

A row whose content sits under its label line at full width, like the prompt template on Input, is a `div` with the `st-block` class holding a `Row` for the label line and the content after it. The `Row` drops its own padding there, and the block draws the hairline like any row.

`LinkRow` is a row that goes somewhere else in Settings, like Layout's `Panes and tracked affects`, which opens Characters. It draws like a `Disclosure` without the open state.

- `label`, `description`, and `anchor` as on `Row`.
- `onClick` runs on press. Call the page's `navigate` there.

`VisuallyHidden` holds text a screen reader reads and the page does not show, like a list row's On or Off.

`useRowIds` returns the ids of the enclosing `Row` for a custom control.

`cx` joins class names.

A few classes in `settings.css` cover small shapes that are not worth a component.

- `st-field-pair` sets two fields in one row 8 px apart, like General's host and port. Give the second field its own `id` and `aria-label`.
- `st-glyph-toggle` is a 28×24 on and off button drawn as text, like the find bar's `Aa`. It carries `aria-pressed`. Add `st-glyph-case` for the Aa weight.
- `st-meta` is quiet 11/15 text beside a heading. `data-tone="danger"` turns it to the danger color for an error, like a failed update check.

## Icons

`ui/icons.tsx` holds the SPEC 6 set. `GearIcon`, `AppearanceIcon`, `LayoutIcon`, `KeyboardIcon`, `BoltIcon`, `UserIcon`, `SearchIcon`, `ChevronRightIcon`, `ChevronDownIcon`, `ChevronUpIcon`, `CloseIcon`, `PlusIcon`, `CheckIcon`, `CopyIcon`, `MoreIcon`, `MinimizeIcon`, and `MaximizeIcon`. Each takes `size` (16 by default, or 12) and `className`. A 12 px icon keeps the 1.25 px stroke.

## Focus and motion

Every control draws a 2 px accent outline 2 px out on keyboard focus. Transitions stop under reduced motion.
