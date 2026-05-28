# Phase 1 Approach Decision

## The constraint to satisfy

> "Default state with no user dock actions must render exactly as the current code does. No positioning changes applied at rest."

Plus:

- No edits to `src/styles.css`.
- No wrappers that change the box model. If unavoidable, `display: contents`.
- No visible drag handles, header bars, tab strips, splitters, or panel chrome.
- Drag = pointerdown + 5px movement; below threshold, click and hover pass through untouched.
- No dockview default styles imported.

## Option A — dockview-react

To satisfy "pixel identical at rest" with dockview, we would have to:

1. Wrap every dockable element in a `DockviewPanel`. dockview owns the panel's outer DOM (`.dv-default-panel`, `.dv-tab`, `.dv-tabs-container`, `.dv-groupview`). At minimum, every panel is rendered inside a `position: absolute` element sized by dockview's grid splitter math.
2. Override every visual surface to match the current look. Even with `singleTabMode: 'fullwidth'` and custom tab components, dockview always renders a tab strip container above the panel content — `display: none` on `.dv-tabs-container` removes it but the parent `.dv-groupview` still applies its own padding/border at `0` only after we override the dockview theme.
3. Replace the natural flex flow inside `.middle` and `.side-panel` with dockview's grid root. dockview's grid sizes panels with absolute positioning even at rest. To get them to look identical to today's flex layout we would have to compute pixel sizes that match what flex produces at every viewport width, every font size, every theme, including when the affects bar auto-hides and when the level row toggles in/out of StatusPane.
4. Still get past the empirical failure mode from the previous attempt: dockview puts panel children inside a React portal, which broke xterm.js's mount lifecycle in Tauri WebKit. Terminal output never appeared, drag/drop never engaged, headers duplicated, dockview's own drop-anchor was hidden by `opacity: 0` in the abyss theme. None of those issues had a clean fix in this environment.

For dockview to pass the constraint, it must produce **zero pixel difference** at rest. It cannot. Even with all default chrome stripped, the panel wrappers still impose absolute positioning whose pixel values would have to be re-derived to match the current flex layout at every viewport size — and any drift means a failed Phase 5 diff.

It also fails the box-model rule directly. dockview's panel wrapper is not `display: contents`-compatible: it relies on its own width/height being authoritative for the splitter system. Forcing `display: contents` on the panel wrapper would break dockview's layout engine.

**Verdict: dockview cannot satisfy the absolute constraint in this codebase.**

## Option B — custom

A custom implementation can satisfy every rule directly:

1. **Default state is the current code.** No wrappers added. Bars render exactly where they do today through the existing flex layout in `.app`, `.middle`, `.side-panel`, `.bottom-rail`. Zero pixel diff guaranteed because nothing about their CSS changes.
2. **Docked state is opt-in per bar.** Only when a bar is dragged into a snap zone does it get a `data-docked="<zone>"` attribute. A new stylesheet (`src/styles.docking.css`) reads only that attribute and applies `position: absolute` + zone coordinates relative to `.middle`. Bars without the attribute get nothing applied.
3. **No new layout containers.** Dockable bars stay in their current parents. When docked, the bar's `position: absolute` lifts it out of its parent's flex flow (its slot collapses to zero height/width), and its rect anchors against `.middle` because `.middle` already has `position: relative`.
4. **Drag detection is plain pointer events on the bar.** A small hook attaches `pointerdown` listeners to bars and watches for 5px movement before initiating drag. Below threshold, the listeners do nothing — clicks reach existing handlers normally because we never call `preventDefault` or `stopPropagation` until the threshold trips.
5. **Drag ghost and snap preview are separate DOM nodes** appended to `document.body` only during a drag, removed on drop or cancel. They cannot affect resting layout because they don't exist at rest.
6. **No imports of any third-party docking library.** Drag, hit-testing, snap math, and persistence are all in our own code (~150-300 lines of TypeScript by my estimate).

The cost is writing the drag/snap/persist code ourselves. The cost we avoid is fighting dockview's mount model in Tauri WebKit (which we already lost once) and fighting its default chrome to get to pixel-identical output (which we cannot win).

**Verdict: custom satisfies every rule and avoids every failure mode from the previous attempt.**

## Recommendation

**Custom implementation.** Default to it per the spec ("Default to custom unless dockview can be proven to add zero visual change") and per the empirical evidence from the previous attempt that dockview cannot make zero visual change in this environment.

## What "custom" looks like in code (preview)

For your reference before approving — no code is being written yet.

- New file: `src/lib/docking.ts`. Exports a `useDockable(ref, id)` hook that bars opt into. The hook attaches pointerdown to `ref.current` and manages drag state via a singleton store.
- New file: `src/components/DockingOverlay.tsx`. Mounted once near the root inside `.middle`. Renders the drag ghost and snap preview when a drag is active; renders nothing at rest.
- New file: `src/styles.docking.css`. Selectors only: `[data-docked="top-left"]`, `[data-docked="top"]`, etc. (and the ghost/preview classes). No selector overlaps any existing class in `styles.css`.
- localStorage key: `mudclient.docking.layout.v1`. Maps bar id → zone string. Loaded on mount; bars without an entry render in default flow.
- Imports into existing components: each dockable bar gets one new line: `useDockable(ref, 'bar-id')`. That call is a no-op until the user actually drags. No JSX, no class name, no wrapper changes.

## Constraint compliance check

| Rule                              | Custom plan                                                          |
| --------------------------------- | -------------------------------------------------------------------- |
| Pixel identical at rest           | Yes — bars use existing flex; hook is a no-op until drag fires.      |
| No edits to existing CSS          | Yes — new file only.                                                 |
| No new layout wrappers            | Yes — no JSX wrappers added; only a hook call inside existing nodes. |
| No visible drag handles or chrome | Yes — the bar IS the drag handle; no glyph, no border change.        |
| No dockview default styles        | Yes — dockview not used.                                             |
| pointerdown + 5px threshold       | Yes — hook implements this directly.                                 |
| Default state unchanged           | Yes — no `data-docked` attributes set until user drags.              |

## Awaiting approval

Approve custom approach? Or push back on any line above (e.g., proposed file paths, localStorage key name, bar id naming scheme)?
