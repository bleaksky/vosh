import { highlightGroundSet } from '../ipc/terminal';
import { getFitGameColors, subscribeFitGameColors } from './fitGameColors';

// Keep highlight colors readable, and Fit game colors. The session lifts
// each fixed color a trigger paints text in, a true color or a 256 color
// past the 16, until it reads on the terminal background, so it needs that
// background while the setting is on. While Fit game colors is on it lifts
// the 256 colors past the 16 the game sends text in on a light background,
// so it needs the background then too, as `game`. Each Terminal reports its
// theme's background here on every theme change, the main window reports
// Keep highlight colors readable, and the fit store reports Fit game
// colors. The command carries each background while its setting is on and
// null while it is off, and goes out only when either changes.

let readable = true;
let ground: string | null = null;
// What the session holds now. Undefined until the first report.
let sent: { background: string | null; game: string | null } | undefined;

function report(): void {
  const background = readable ? ground : null;
  const game = getFitGameColors() ? ground : null;
  if (sent?.background === background && sent.game === game) return;
  sent = { background, game };
  void highlightGroundSet({ background, game }).catch(() => {});
}

subscribeFitGameColors(report);

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
