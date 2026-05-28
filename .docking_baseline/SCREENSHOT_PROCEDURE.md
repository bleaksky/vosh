# Phase 0 baseline screenshot procedure

The Phase 5 visual regression check pixel-diffs `.docking_baseline/` against `.docking_after/`. To make that diff meaningful these baselines have to capture the _exact_ state we will reset to in Phase 5: clean localStorage, default theme, default font, no connection, no drawers, side panel visible.

## Pre-capture reset

1. Quit the Tauri dev app fully.
2. In a terminal: `npm run tauri dev` from the repo root.
3. Once the window opens, open devtools (right click → Inspect, or Cmd+Option+I) and run in the console:

   ```js
   localStorage.clear();
   location.reload();
   ```

4. Confirm:
   - Connect bar at top with default `play.theforsakenlands.com:1848` fields, not connected.
   - Terminal column on the left.
   - Side panel on the right with status / affects / map / info-tabs sections.
   - Bottom rail with input prompt + status bar (HP/MN/MV showing `-`/`-`).
   - Affects bar is hidden if no tracked affects are configured (this is fine; default is empty).
   - Theme is the default Kanso Zen dark.

## Capture (macOS)

Use `screencapture -l <window_id>` so the screenshot is exactly the app window's content area, with no desktop chrome around it. To find the window id:

```bash
osascript -e 'tell application "System Events" to get the id of every window of process "mudclient"'
```

(or run `osascript -e 'tell app "System Events" to get name of every process whose visible is true'` to confirm the process name; the dev build may show as `tauri-app` or similar — adjust accordingly).

### Default viewport (1280×800)

Resize the window to roughly 1280×800. Then:

```bash
screencapture -l <window_id> /Users/james/local/mudclient/.docking_baseline/01-default-1280x800.png
```

### Narrow (900×800)

Drag the window narrow (about 900 wide). The Resizable side panel may auto-shrink — let it. Then:

```bash
screencapture -l <window_id> /Users/james/local/mudclient/.docking_baseline/02-narrow-900x800.png
```

### Wide (1800×1100)

Resize wide (about 1800×1100). Then:

```bash
screencapture -l <window_id> /Users/james/local/mudclient/.docking_baseline/03-wide-1800x1100.png
```

## Verify

```bash
ls -la /Users/james/local/mudclient/.docking_baseline/*.png
```

Three PNGs should be present. Tell me they're captured and I'll move on to Phase 1.

## Why I'm not doing this myself

I have no GUI control in this session — I cannot resize the Tauri window, click the dev menu, or trigger `screencapture` against a window I cannot focus. Anything I tried (e.g. firing `screencapture` blindly) would either capture the wrong window or my own terminal. Better that you take the three shots so the baseline reflects exactly what you see, since "pixel identical" is judged against your screen.
