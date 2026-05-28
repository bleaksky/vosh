# Phase 0 Layout Map

Captured against the current `main` (commit `107a190`, post-dockview revert).

## Top-level DOM tree

```
<main class="app">                          // flex column, height 100vh
  <Connect class="connect" />               // flex 0 0 auto, top bar
  <div class="middle">                      // flex 1 1 0, flex row, min-height 0
    <div class="terminal-column">           // flex 1 1 0
      <Terminal />                          //   .terminal-sizer > .terminal-host (xterm)
    </div>
    {sidePanelOpen && (
      <Resizable storageKey="...sidePanelWidth" defaultWidth={320}>
        <SidePanel class="side-panel">      // flex 0 0 column on the right
          <StatusPane class="status-pane" />     // flex 0 0 auto, .pane-header "status"
          <AffectsPane class="affects-pane" />   // flex 0 0 auto, .pane-header "affects", max-height 30vh
          <MapPane class="map-pane" />           // flex 0 0 280px, .pane-header "map"
          <InfoTabsPane class="info-tabs-pane" />// flex 0 0 auto, max-height 26vh, internal tab strip
        </SidePanel>
      </Resizable>
    )}
    {triggersOpen && <Resizable><TriggersDrawer /></Resizable>}
    {settingsOpen && <Resizable><SettingsDrawer /></Resizable>}
    {searchOpen && <Resizable><SearchView /></Resizable>}
  </div>
  <div class="bottom-rail">                 // flex 0 0 auto, flex column, full width
    <Input class="input-row" />             //   prompt + text field
    <StatusBar class="statusbar" />         //   HP/MN/MV bars + tick + time + sky/weather + moons
    <AffectsBar class="affects-bar" />      //   tracked affect pills (hidden when none configured)
  </div>
</main>
```

## Bar inventory

These are the elements the user can drag onto a snap zone in later phases. Each row lists the component, its rendered class, current parent, current sizing, current spacing, and whether it carries any internal interactive controls that drag detection must NOT swallow.

| #   | Component       | Class              | Current parent | Sizing                                             | Spacing                                                         | Interactive content                                             |
| --- | --------------- | ------------------ | -------------- | -------------------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------------------- |
| 1   | StatusPane      | `.status-pane`     | `.side-panel`  | flex 0 0 auto                                      | `.pane-header` 0.4rem 0.75rem; body 0.6rem 0.75rem, gap 0.45rem | none (read-only rows)                                           |
| 2   | AffectsPane     | `.affects-pane`    | `.side-panel`  | flex 0 0 auto, max-height 30vh                     | header 0.4rem 0.75rem; body 0.35rem 0                           | row buttons toggle expand/collapse — clicks must still register |
| 3   | MapPane         | `.map-pane`        | `.side-panel`  | flex 0 0 280px                                     | header 0.4rem 0.75rem; canvas fills remainder                   | mapping/server toggle in header; canvas accepts wheel/drag pans |
| 4   | InfoTabsPane    | `.info-tabs-pane`  | `.side-panel`  | flex 0 0 auto, max-height 26vh                     | tab strip + body, internal scroll                               | chat/gold/cabal tab buttons                                     |
| 5   | StatusBar       | `.statusbar`       | `.bottom-rail` | flex 0 0 auto                                      | row of `.statusbar-bar`, cells, divider, moons                  | none (display-only)                                             |
| 6   | AffectsBar      | `.affects-bar`     | `.bottom-rail` | flex 0 0 auto (auto-hides when no tracked affects) | pill row                                                        | pills carry tooltips (`title=`); no clicks                      |
| 7   | Input           | `.input-row`       | `.bottom-rail` | flex 0 0 auto                                      | 0.15rem 0.55rem, prompt + text input                            | text input — must remain fully usable                           |
| 8   | Connect         | `.connect`         | `.app` (top)   | flex 0 0 auto                                      | 0.5rem 0.75rem, host/port/tls/connect + drawer toggles          | several form inputs and buttons                                 |
| 9   | Terminal column | `.terminal-column` | `.middle`      | flex 1 1 0                                         | xterm fills sizer                                               | text selection, scrollback, mouse wheel                         |

Whether items 7/8/9 are user-draggable in later phases is undecided. The hard rule is "look pixel identical at rest" so they remain on this list as candidates only; we will narrow scope in Phase 1.

## Dock-root candidates

The "dock root" is the parent rect that snap zones (corners, edges) are computed against. There are two viable choices:

1. **`.middle`** (`src/styles.css:122`) — the flex row that already holds the terminal and the optional side panel. Width = viewport - 0; height = viewport - connect bar - bottom rail. Already `position: relative` (line 132) and `overflow: hidden` (line 131). Snap zones here would attach docked bars to the inside edges/corners of the terminal-and-side-panel area, leaving the connect bar and bottom rail untouched.
2. **`.app`** (`src/styles.css:27`) — the outer flex column covering the full viewport. Snap zones here would let docked bars attach against the very edges of the window, overlapping the connect bar and bottom rail.

`.middle` is the safer default because:

- It already has `position: relative` (no new property needed → no risk of shifting any descendant).
- Its rect equals the natural "play area" of the client.
- Bars docked against its top edge sit just below the connect bar; bars docked against its bottom edge sit just above the bottom rail. Both line up cleanly with existing chrome.
- Snap math stays simple: corners and edges are computed against a single rect that doesn't change as connect/rail heights change.

`.app` would force overlap with the connect bar and bottom rail, which the user has explicitly tied to the bottom of the viewport ("the bottom rail anchors to the bottom of the viewport without any positioning trickery" — App.tsx:213-216 comment).

**Proposed dock root: `.middle`** (subject to your approval in the Phase 0 gate).

## Positioning rules summary

- Everything is flex today. No element uses `position: absolute` for layout. `.terminal-host` does, but only inside its own `.terminal-sizer`.
- `.middle` is the only ancestor with `position: relative` set explicitly.
- The side panel is itself a flex column inside `.middle`; bars docked into the right edge of `.middle` would visually live in the same column as today's `.side-panel`, so a "right edge" dock target has to coexist with the existing side panel rather than replace it.

## CSS files in scope

Only one stylesheet exists today: `src/styles.css`. Per the hard rules, Phase 2+ work introduces a new file (proposed: `src/styles.docking.css`) that is loaded only by the new docking module and contains nothing but docking-specific selectors (drag ghost, snap preview outline, docked-bar absolute positioning). No edits to `styles.css`.

## What the user must do

I cannot drive the Tauri window from this environment. The baseline screenshots have to be captured by you. Procedure below; the directory `.docking_baseline/` is created and ready.
