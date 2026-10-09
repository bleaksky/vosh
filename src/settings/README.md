# Settings

`src/settings/SettingsWindow.tsx` is the frame. It draws the sidebar (`Sidebar.tsx`), the breadcrumb band, and one page per group. Each page lives in a folder of its own, `general/`, `appearance/`, `layout/`, `input/`, `automation/`, `scripts/`, and `characters/`. Every page is built from its board and from the primitives in `src/ui`, which `src/ui/README.md` lists with their props.

## Pages

A page is the component its folder is named for, like `layout/LayoutPage.tsx`, and takes `SettingsPageProps` from `pageTypes.ts`.

- `target` is where the page should land. `target.section` and `target.anchor` come from the nav, a deep link, or a search hit.
- `navSeq` goes up on every navigation, even to the same target. React to it when the target changes state on the page, like the Automation kind or the Characters profile.
- `config`, `setConfig`, and `onError` are the window's UiConfig copy, its setter, and the error line above the page. Never keep a second copy of the config, since every page shows this one and each save reads what a field held from it.
- `pathB` is true in loadout mode.
- `navigate(target)` goes somewhere else in Settings.
- `setLeaveGuard(guard)` registers a question the frame asks before it moves to another group, or to another page inside the group, like the crumb back to Scripts from a plugin's page. The guard gets a `proceed` callback and returns true to hold the move, then calls `proceed` once you confirm. Automation and a plugin's page use it to ask before they drop unsaved changes. Clear it with null when the page unmounts. To ask the same before the window closes, a page calls `useCloseGuard` from `useCloseGuard.ts`, beside the frame.

Register a page in `PAGES` in `SettingsWindow.tsx`. A page that pins its own bar and scrolls inside itself, like the Automation save bar, sets `selfScroll` there. `selfScroll` can also be a function of the target, for a group where only some targets scroll on their own.

A group can hold a page inside it, like the session logs at `general:logs`. Name it in `SETTINGS_SUBPAGES` in `src/lib/settingsNav.ts` with its title. The breadcrumb then reads `Settings › General › Session logs` with the group as a link back, the nav keeps the group active, and the frame does not scroll to the section. The group's page draws the inner page when `settingsSubpage(target)` names it. Under Scripts every section is a page inside the group, a plugin's own page, titled with the plugin's name, so `scripts:vitals_alert` reads `Settings › Scripts › vitals_alert`.

A page that saves as you go takes `update` from `useSettingsAutoSave`, beside the frame. `update(patch)` patches the config copy and saves the fields the patch names once typing settles. Pass `{ now: true }` for a discrete pick another window shows at once, like a theme or a toggle.

`characters/CharactersPage.tsx` is the Characters board and `appearance/AppearancePage.tsx` the Appearance board, each with its parts beside it. Import… under the profile list reads a Vosh profile export, and `characters/ImportSheet.tsx` takes the detail column while you choose where it goes, with its words from `characters/profileImport.ts`. The split divider color lives only on Layout and the sent command color only on Input, so Appearance's Advanced does not show them. `automation/AutomationPage.tsx` is the Automation board, described under Automation below.

`scripts/ScriptsPage.tsx` is the list page of the Scripts boards, your plugins in `scripts/PluginList.tsx` and the Console in `scripts/LuaConsole.tsx`. It reads the plugins and the Lua lines as it opens and keeps them itself, following `vosh://plugins-changed`, `vosh://profile-switched` and `session://lua-output`. New plugin opens `scripts/NewPluginDialog.tsx`, and a press on a row opens the plugin's page, `scripts/PluginPage.tsx`. Each row's more menu, `scripts/PluginMenu.tsx`, reloads, shows, exports and removes its plugin. Install reads a .zip you pick or a folder you drop on the list page through `scripts/pluginPackage.ts` and asks once in `scripts/InstallDialog.tsx`. That page edits a draft of the file the plugin runs first in the page surface of `CodeEditor` and of its manifest in `scripts/ManifestCard.tsx`, shows its stop note and its Output, the Console section with that plugin's lines, and saves through `plugin_save`. What it says about the plugin, the stop note, the line its newest error marks and the save bar's time, comes from `scripts/pluginState.ts`.

`general/GeneralPage.tsx` is the General board, with the session log view in `general/SessionLogs.tsx` and Save a scene in `general/ScenePage.tsx` at `general:scene`. Save a scene… in the log view opens the scene page on the log it picked, and the terminal's menu and search open it on the selected session's newest log. Its words and its range live in `general/scene.ts`. `input/InputPage.tsx` and `layout/LayoutPage.tsx` are the Input and Layout boards. Layout's Status line section holds `layout/TickTimeStyleRow.tsx`, the Tick and time row, and under it `layout/TickCountRow.tsx`, the Tick counts row.

## Deep links and search

A deep link is a string like `automation:macros` or `characters:Ilsabet#tracked`. `src/lib/settingsNav.ts` resolves it, and a string it cannot read opens General. `src/lib/settingsLink.ts` opens Settings on one from the main window.

What a section means depends on the group. In Automation it is the kind. In Characters it is the profile name, and no section means the active profile. In Scripts it is a plugin name, and no section means the list. Profile and plugin names keep their case. Everywhere else it is a section `id` the frame scrolls to.

Search finds rows. `settingsSearch.ts` lists every row with its label and target. When a page adds a row, add it there too, and give the element the same anchor, `anchor` on `Row` or `Disclosure` and `id` on `Section`. The frame scrolls to it and flashes a row (`revealAnchor.ts`). When the anchor sits inside a closed `Disclosure`, open it when `target.anchor` names it.

## Automation

`automation/AutomationPage.tsx` holds the kind switcher, `Import…`, and the discard question. Each kind is an editor beside it built on `DraftEditor`, which draws the list, the detail card, and the save bar over a draft from `src/automation/automationDraft.ts`. A kind is a `KindSpec` (`automation/types.ts`): how it loads, saves through the kind's existing API, validates, lists, and draws its detail card. Timers pins the Tick above its list with a draft of its own. Every kind loads and saves the profile Settings shows, which `shownProfile.ts` keeps, and a draft with unsaved changes holds that profile while the selection moves to a session on another one. The pure logic, with tests, sits in `src/automation/automationDraft.ts`, `automationList.ts`, `automationTriggers.ts`, `automationRecords.ts`, and `alertParts.ts`. `automation/AlertRows.tsx` draws the Alert row of the trigger card and the four rows that tune the alert under its Advanced.

Save writes only what the draft added, changed, and removed. Triggers and aliases go through one call that replaces the whole store, so their save reads the store again first and applies the draft over it by name (`saveTriggerDraft`, `saveAliasDraft`, `mergeDraftChanges`). Macros and timers save item by item, and a timer id always comes from the load. The backend sends `vosh://triggers-changed` and `vosh://aliases-changed` whenever a list changes, from Settings, #trigger, #alias, Lua, an import, or a preset (`src-tauri/src/app/events.rs`). A clean draft follows the store at once. A dirty one keeps your edits, says the list changed, and catches up once you save or discard.

The page draws the anchors `tick` (the Tick row), `import` (the import section), and `json` (`Edit all as JSON…` and the JSON view). A target with the `import` or `json` anchor opens that view, and `tick` selects the Tick.
