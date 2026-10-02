import { invoke } from '@tauri-apps/api/core';

// Keep highlight colors readable. The session lifts each true color a
// trigger paints text in until it reads on the terminal background, so it
// needs that background while the setting is on. Each Terminal reports its
// theme's background here on every theme change, and the main window
// reports the setting. The command carries the background while the
// setting is on and null while it is off, and goes out only when that
// changes.

let readable = true;
let ground: string | null = null;
// What the session holds now. Undefined until the first report.
let sent: string | null | undefined;

function report(): void {
  const next = readable ? ground : null;
  if (next === sent) return;
  sent = next;
  void invoke('highlight_ground_set', { background: next }).catch(() => {});
}

/** The Keep highlight colors readable setting changed, or loaded. */
export function setReadableHighlights(on: boolean): void {
  readable = on;
  report();
}

/** The theme's terminal background, `#rrggbb`, changed or loaded. */
export function setHighlightGround(background: string | undefined): void {
  ground = background ?? null;
  report();
}

/** Forget what was sent, for tests. */
export function resetHighlightGround(): void {
  readable = true;
  ground = null;
  sent = undefined;
}
