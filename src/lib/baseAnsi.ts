// The base terminal palette: the 16 ANSI colors used whenever tint
// output with theme is OFF. Defaults to the canonical xterm-256 chart
// so server output reads like a stock xterm; the themes tab lets the
// user replace any slot, stored as ui.terminal_base_ansi. The module
// keeps the live override so both terminal renderers re-derive their
// palette without a relaunch.

/** Slot order matches ANSI 0-15. */
export const ANSI_SLOTS = [
  'black',
  'red',
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'white',
  'brightBlack',
  'brightRed',
  'brightGreen',
  'brightYellow',
  'brightBlue',
  'brightMagenta',
  'brightCyan',
  'brightWhite',
] as const;

export type AnsiSlot = (typeof ANSI_SLOTS)[number];

// Values match the SVG chart at
// https://upload.wikimedia.org/wikipedia/commons/1/15/Xterm_256color_chart.svg
export const CANONICAL_ANSI_16: Record<AnsiSlot, string> = {
  black: '#000000',
  red: '#800000',
  green: '#008000',
  yellow: '#808000',
  blue: '#000080',
  magenta: '#800080',
  cyan: '#008080',
  white: '#c0c0c0',
  brightBlack: '#808080',
  brightRed: '#ff0000',
  brightGreen: '#00ff00',
  brightYellow: '#ffff00',
  brightBlue: '#0000ff',
  brightMagenta: '#ff00ff',
  brightCyan: '#00ffff',
  brightWhite: '#ffffff',
};

let override: Record<AnsiSlot, string> | null = null;
const subs = new Set<() => void>();

/** Accepts the persisted 16-entry hex list (ANSI order) or null for
 *  canonical. Invalid shapes fall back to canonical. */
export function setBaseAnsi(colors: string[] | null): void {
  let next: Record<AnsiSlot, string> | null = null;
  if (Array.isArray(colors) && colors.length === ANSI_SLOTS.length) {
    const record = {} as Record<AnsiSlot, string>;
    let valid = true;
    ANSI_SLOTS.forEach((slot, i) => {
      const v = colors[i];
      if (typeof v === 'string' && v.trim().length > 0) record[slot] = v.trim();
      else valid = false;
    });
    if (valid) next = record;
  }
  const changed = JSON.stringify(next) !== JSON.stringify(override);
  override = next;
  if (changed) for (const cb of subs) cb();
}

/** The active base palette record (canonical unless overridden). */
export function baseAnsiRecord(): Record<AnsiSlot, string> {
  return override ?? CANONICAL_ANSI_16;
}

/** The active base palette as the persisted 16-entry list, or null
 *  when it is the canonical default. */
export function baseAnsiList(): string[] | null {
  return override ? ANSI_SLOTS.map((s) => override![s]) : null;
}

export function subscribeBaseAnsi(cb: () => void): () => void {
  subs.add(cb);
  return () => {
    subs.delete(cb);
  };
}
