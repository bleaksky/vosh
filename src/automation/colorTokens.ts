// Friendly color token grammar shared by the trigger preset library
// and the trigger form editor. Tokens look like {red}, {bold_red},
// {fg:244}, {bg:244}, {#ff3399}, {reset}. colorize() expands tokens
// to raw ANSI escapes; decolorize() does the inverse so already-
// colorized templates round-trip as friendly tokens in the form.

const NAMED: Record<string, string> = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  underline: '\x1b[4m',
  black: '\x1b[30m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  white: '\x1b[37m',
  bright_black: '\x1b[90m',
  bright_red: '\x1b[91m',
  bright_green: '\x1b[92m',
  bright_yellow: '\x1b[93m',
  bright_blue: '\x1b[94m',
  bright_magenta: '\x1b[95m',
  bright_cyan: '\x1b[96m',
  bright_white: '\x1b[97m',
  bold_red: '\x1b[1;31m',
  bold_green: '\x1b[1;32m',
  bold_yellow: '\x1b[1;33m',
  bold_blue: '\x1b[1;34m',
  bold_magenta: '\x1b[1;35m',
  bold_cyan: '\x1b[1;36m',
  bold_white: '\x1b[1;37m',
};

// Inverse lookup: ANSI escape → token name.
const NAMED_INVERSE: Record<string, string> = Object.fromEntries(
  Object.entries(NAMED).map(([k, v]) => [v, k]),
);

function hexToRgb(hex: string): [number, number, number] | null {
  let h = hex.startsWith('#') ? hex.slice(1) : hex;
  if (h.length === 3) {
    h = h
      .split('')
      .map((c) => c + c)
      .join('');
  }
  if (h.length !== 6) return null;
  const n = parseInt(h, 16);
  if (Number.isNaN(n)) return null;
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

function rgbToHex(r: number, g: number, b: number): string {
  const toHex = (n: number) => Math.max(0, Math.min(255, n)).toString(16).padStart(2, '0');
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

/// Expand friendly tokens into raw ANSI. Idempotent on already-
/// expanded escapes (anything that doesn't match `{...}` passes
/// through verbatim).
export function colorize(template: string): string {
  let out = template;
  out = out.replace(/\{fg:(\d+)\}/g, (_, n) => `\x1b[38;5;${n}m`);
  out = out.replace(/\{bg:(\d+)\}/g, (_, n) => `\x1b[48;5;${n}m`);
  out = out.replace(/\{#([0-9a-fA-F]{3,6})\}/g, (m, hex) => {
    const rgb = hexToRgb(hex);
    return rgb ? `\x1b[38;2;${rgb[0]};${rgb[1]};${rgb[2]}m` : m;
  });
  out = out.replace(/\{bg#([0-9a-fA-F]{3,6})\}/g, (m, hex) => {
    const rgb = hexToRgb(hex);
    return rgb ? `\x1b[48;2;${rgb[0]};${rgb[1]};${rgb[2]}m` : m;
  });
  out = out.replace(/\{([a-z_]+)\}/g, (m, name) => NAMED[name] ?? m);
  return out;
}

/// Inverse of colorize. Replaces every ANSI escape sequence we know
/// with a friendly token; unknown escapes stay verbatim. Used by the
/// trigger form so already-colorized templates display readably.
export function decolorize(raw: string): string {
  let out = raw;
  // Named/single-code forms first (they're the most common).
  for (const [escape, name] of Object.entries(NAMED_INVERSE)) {
    out = out.split(escape).join(`{${name}}`);
  }
  // 256-color foreground / background.
  // eslint-disable-next-line no-control-regex
  out = out.replace(/\x1b\[38;5;(\d+)m/g, (_, n) => `{fg:${n}}`);
  // eslint-disable-next-line no-control-regex
  out = out.replace(/\x1b\[48;5;(\d+)m/g, (_, n) => `{bg:${n}}`);
  // 24-bit truecolor → hex.
  out = out.replace(
    // eslint-disable-next-line no-control-regex
    /\x1b\[38;2;(\d+);(\d+);(\d+)m/g,
    (_, r, g, b) => `{${rgbToHex(Number(r), Number(g), Number(b))}}`,
  );
  out = out.replace(
    // eslint-disable-next-line no-control-regex
    /\x1b\[48;2;(\d+);(\d+);(\d+)m/g,
    (_, r, g, b) => `{bg${rgbToHex(Number(r), Number(g), Number(b))}}`,
  );
  return out;
}
