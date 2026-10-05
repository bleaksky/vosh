// Theme file import. Reads the color scheme files other terminals ship
// (Ghostty, iTerm2, Kitty, and Alacritty) into Vosh's CustomTheme
// shape. Only the terminal palette comes across. lib/chrome derives the
// window chrome from it, the same way it dresses every built-in theme.
//
// Each reader is small and forgiving. It skips keys it does not know,
// so a whole terminal config file works as well as a bare theme, and it
// only fails when the 16 ANSI colors, the background, or the foreground
// are missing.

import type { CustomTheme } from '../ipc/uiConfig';

export type ThemeFileFormat = 'ghostty' | 'iterm2' | 'kitty' | 'alacritty-toml' | 'alacritty-yaml';

export const MISSING_COLORS_MESSAGE =
  'Vosh could not read that theme file. It needs 16 colors, a background, and a foreground.';
export const UNKNOWN_FORMAT_MESSAGE =
  'Vosh could not read that theme file. It reads Ghostty, iTerm2, Kitty, and Alacritty themes.';

/** A theme file Vosh cannot use. The message is ready to show. */
export class ThemeFileError extends Error {
  override name = 'ThemeFileError';
}

const FORMAT_LABELS: Record<ThemeFileFormat, string> = {
  ghostty: 'Ghostty',
  iterm2: 'iTerm2',
  kitty: 'Kitty',
  'alacritty-toml': 'Alacritty',
  'alacritty-yaml': 'Alacritty',
};

/** The 16 ANSI slots in the order the xterm palette names them. */
const ANSI_KEYS = [
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

/** Alacritty names the eight base colors the same in both halves. */
const BASE_NAMES = ANSI_KEYS.slice(0, 8);

type SurfaceKey =
  | 'background'
  | 'foreground'
  | 'cursor'
  | 'cursorAccent'
  | 'selectionBackground'
  | 'selectionForeground';

interface Slots {
  surfaces: Partial<Record<SurfaceKey, string>>;
  ansi: Array<string | undefined>;
  /** A name the file gives itself, when it has one. */
  name?: string;
}

function emptySlots(): Slots {
  return { surfaces: {}, ansi: new Array<string | undefined>(16).fill(undefined) };
}

function setSurface(slots: Slots, key: SurfaceKey, raw: string | undefined): void {
  const color = raw === undefined ? null : normalizeColor(raw);
  if (color) slots.surfaces[key] = color;
}

function setAnsi(slots: Slots, index: number, raw: string | undefined): void {
  if (!Number.isInteger(index) || index < 0 || index > 15) return;
  const color = raw === undefined ? null : normalizeColor(raw);
  if (color) slots.ansi[index] = color;
}

function unquote(value: string): string {
  const v = value.trim();
  if (v.length >= 2 && (v[0] === '"' || v[0] === "'") && v[v.length - 1] === v[0]) {
    return v.slice(1, -1);
  }
  return v;
}

const hex2 = (n: number) =>
  Math.max(0, Math.min(255, Math.round(n)))
    .toString(16)
    .padStart(2, '0');

/** A color as lowercase #rrggbb, or null when the value is not one.
 *  Reads #rgb, #rrggbb, #rrggbbaa (alpha dropped), the same without the
 *  hash, 0xrrggbb, and X11 rgb:r/g/b. */
export function normalizeColor(raw: string): string | null {
  let v = unquote(raw);
  const x11 = /^rgb:([0-9a-f]{1,4})\/([0-9a-f]{1,4})\/([0-9a-f]{1,4})$/i.exec(v);
  if (x11) {
    const channel = (h: string) => (parseInt(h, 16) / (16 ** h.length - 1)) * 255;
    return `#${hex2(channel(x11[1]))}${hex2(channel(x11[2]))}${hex2(channel(x11[3]))}`;
  }
  if (/^0x[0-9a-f]{6}$/i.test(v)) v = v.slice(2);
  else if (v.startsWith('#')) v = v.slice(1);
  if (/^[0-9a-f]{3}$/i.test(v)) v = [...v].map((c) => c + c).join('');
  if (/^[0-9a-f]{8}$/i.test(v)) v = v.slice(0, 6);
  return /^[0-9a-f]{6}$/i.test(v) ? `#${v.toLowerCase()}` : null;
}

function lines(text: string): string[] {
  return text.replace(/^\uFEFF/, '').split(/\r?\n/);
}

// ── Ghostty ─────────────────────────────────────────────────────────
// `key = value` lines, comments on their own lines, and one
// `palette = N=#color` line per ANSI slot.

const GHOSTTY_KEYS: Record<string, SurfaceKey> = {
  background: 'background',
  foreground: 'foreground',
  'cursor-color': 'cursor',
  'cursor-text': 'cursorAccent',
  'selection-background': 'selectionBackground',
  'selection-foreground': 'selectionForeground',
};

function readGhostty(text: string): Slots {
  const slots = emptySlots();
  for (const line of lines(text)) {
    const t = line.trim();
    if (t === '' || t.startsWith('#')) continue;
    const eq = t.indexOf('=');
    if (eq < 0) continue;
    const key = t.slice(0, eq).trim().toLowerCase();
    const value = unquote(t.slice(eq + 1));
    if (key === 'palette') {
      const m = /^(\d+)\s*=\s*(.+)$/.exec(value);
      if (m) setAnsi(slots, Number(m[1]), m[2]);
      continue;
    }
    const surface = GHOSTTY_KEYS[key];
    if (surface) setSurface(slots, surface, value);
  }
  return slots;
}

// ── iTerm2 ──────────────────────────────────────────────────────────
// An XML property list. Each color is a key followed by a dict of float
// components from 0 to 1. Newer exports also carry `(Light)` and
// `(Dark)` variants, and the dark one stands in when the plain key is
// missing.

function readIterm(text: string): Slots {
  const slots = emptySlots();
  const colors = new Map<string, string>();
  const entry = /<key>([^<]+)<\/key>\s*<dict>([\s\S]*?)<\/dict>/g;
  for (let m = entry.exec(text); m; m = entry.exec(text)) {
    const body = m[2];
    const component = (label: string) => {
      const c = new RegExp(
        `<key>${label} Component</key>\\s*<(?:real|integer)>([^<]+)</(?:real|integer)>`,
      ).exec(body);
      return c ? Number(c[1]) : NaN;
    };
    const rgb = [component('Red'), component('Green'), component('Blue')];
    if (rgb.every(Number.isFinite)) {
      colors.set(m[1].trim(), `#${rgb.map((c) => hex2(c * 255)).join('')}`);
    }
  }
  const get = (key: string) => colors.get(key) ?? colors.get(`${key} (Dark)`);
  setSurface(slots, 'background', get('Background Color'));
  setSurface(slots, 'foreground', get('Foreground Color'));
  setSurface(slots, 'cursor', get('Cursor Color'));
  setSurface(slots, 'cursorAccent', get('Cursor Text Color'));
  setSurface(slots, 'selectionBackground', get('Selection Color'));
  setSurface(slots, 'selectionForeground', get('Selected Text Color'));
  for (let i = 0; i < 16; i += 1) setAnsi(slots, i, get(`Ansi ${i} Color`));
  return slots;
}

// ── Kitty ───────────────────────────────────────────────────────────
// `key value` lines separated by whitespace, `color0` to `color15` for
// the ANSI slots, and an optional `## name:` header in the theme files
// kitty-themes ships.

const KITTY_KEYS: Record<string, SurfaceKey> = {
  background: 'background',
  foreground: 'foreground',
  cursor: 'cursor',
  cursor_text_color: 'cursorAccent',
  selection_background: 'selectionBackground',
  selection_foreground: 'selectionForeground',
};

function readKitty(text: string): Slots {
  const slots = emptySlots();
  const name = /^\s*##\s*name\s*:\s*(.+?)\s*$/im.exec(text);
  if (name) slots.name = name[1];
  for (const line of lines(text)) {
    const t = line.trim();
    if (t === '' || t.startsWith('#')) continue;
    const m = /^(\S+)\s+(\S+)/.exec(t);
    if (!m) continue;
    const key = m[1].toLowerCase();
    const ansi = /^color(\d{1,3})$/.exec(key);
    if (ansi) {
      setAnsi(slots, Number(ansi[1]), m[2]);
      continue;
    }
    const surface = KITTY_KEYS[key];
    if (surface) setSurface(slots, surface, m[2]);
  }
  return slots;
}

// ── Alacritty ───────────────────────────────────────────────────────
// Both the TOML config and the legacy YAML one nest the palette under
// colors.primary, colors.cursor, colors.selection, colors.normal, and
// colors.bright. Each reader flattens its file to dotted paths, and one
// lookup reads the palette from either.

/** Drop a trailing comment that sits outside quotes. */
function stripComment(line: string): string {
  let quote = '';
  for (let i = 0; i < line.length; i += 1) {
    const c = line[i];
    if (quote) {
      if (c === quote) quote = '';
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === '#') {
      return line.slice(0, i);
    }
  }
  return line;
}

function flattenToml(text: string): Map<string, string> {
  const out = new Map<string, string>();
  let table = '';
  for (const line of lines(text)) {
    const t = stripComment(line).trim();
    if (t === '') continue;
    const header = /^\[\[?\s*([^\]]+?)\s*\]\]?$/.exec(t);
    if (header) {
      table = header[1].replace(/["'\s]/g, '');
      continue;
    }
    const eq = t.indexOf('=');
    if (eq < 0) continue;
    const key = t.slice(0, eq).replace(/["'\s]/g, '');
    const value = t.slice(eq + 1).trim();
    const path = table ? `${table}.${key}` : key;
    const inline = /^\{(.*)\}$/.exec(value);
    if (inline) {
      for (const pair of inline[1].split(',')) {
        const peq = pair.indexOf('=');
        if (peq < 0) continue;
        out.set(
          `${path}.${pair.slice(0, peq).replace(/["'\s]/g, '')}`,
          unquote(pair.slice(peq + 1)),
        );
      }
      continue;
    }
    out.set(path, unquote(value));
  }
  return out;
}

function flattenYaml(text: string): Map<string, string> {
  const out = new Map<string, string>();
  const stack: Array<{ indent: number; key: string }> = [];
  for (const line of lines(text)) {
    if (line.trim() === '' || line.trim().startsWith('#')) continue;
    const m = /^(\s*)([^:#]+?)\s*:(?:\s+(.*))?$/.exec(line);
    if (!m) continue;
    const indent = m[1].length;
    while (stack.length > 0 && stack[stack.length - 1].indent >= indent) stack.pop();
    const key = unquote(m[2]);
    let value = (m[3] ?? '').trim();
    // YAML reads an unquoted #rrggbb as a comment. Theme authors mean the
    // color, so keep a bare hex color and drop any other comment.
    if (!/^#[0-9a-f]{3,8}\b/i.test(value)) value = stripComment(value).trim();
    else value = value.split(/\s/)[0];
    if (value === '') {
      stack.push({ indent, key });
      continue;
    }
    out.set([...stack.map((s) => s.key), key].join('.'), unquote(value));
  }
  return out;
}

function readAlacritty(paths: Map<string, string>): Slots {
  const slots = emptySlots();
  const get = (suffix: string) => {
    for (const [path, value] of paths) {
      if (path === suffix || path.endsWith(`.${suffix}`)) return value;
    }
    return undefined;
  };
  setSurface(slots, 'background', get('primary.background'));
  setSurface(slots, 'foreground', get('primary.foreground'));
  // Alacritty also takes CellForeground and CellBackground here, which
  // are not colors, so those fall back like a missing value.
  setSurface(slots, 'cursor', get('cursor.cursor'));
  setSurface(slots, 'cursorAccent', get('cursor.text'));
  setSurface(slots, 'selectionBackground', get('selection.background'));
  setSurface(slots, 'selectionForeground', get('selection.text'));
  BASE_NAMES.forEach((name, i) => {
    setAnsi(slots, i, get(`normal.${name}`));
    setAnsi(slots, i + 8, get(`bright.${name}`));
  });
  return slots;
}

// ── Detection and assembly ──────────────────────────────────────────

/** Which reader a file needs, from its contents first and then its
 *  extension. Null when nothing matches. */
export function detectThemeFormat(name: string, text: string): ThemeFileFormat | null {
  if (/<plist[\s>]|<key>Ansi \d+ Color/.test(text) || text.startsWith('bplist')) return 'iterm2';
  if (/^\s*\[\s*colors[.\s\]]/m.test(text) || /^\s*colors\.\w+\s*=/m.test(text)) {
    return 'alacritty-toml';
  }
  if (/^colors\s*:\s*(#.*)?$/m.test(text)) return 'alacritty-yaml';
  if (/^\s*palette\s*=\s*\d+\s*=/m.test(text)) return 'ghostty';
  if (/^\s*color\d{1,3}\s+\S/m.test(text)) return 'kitty';
  if (/^\s*(background|foreground)\s*=/m.test(text)) return 'ghostty';
  if (/^\s*(background|foreground)\s+\S/m.test(text)) return 'kitty';
  const ext = /\.([a-z]+)$/i.exec(name)?.[1]?.toLowerCase();
  if (ext === 'itermcolors') return 'iterm2';
  if (ext === 'toml') return 'alacritty-toml';
  if (ext === 'yml' || ext === 'yaml') return 'alacritty-yaml';
  if (ext === 'conf') return 'kitty';
  return null;
}

function readSlots(format: ThemeFileFormat, text: string): Slots {
  switch (format) {
    case 'ghostty':
      return readGhostty(text);
    case 'iterm2':
      return readIterm(text);
    case 'kitty':
      return readKitty(text);
    case 'alacritty-toml':
      return readAlacritty(flattenToml(text));
    case 'alacritty-yaml':
      return readAlacritty(flattenYaml(text));
  }
}

/** A display name from a file name. `tokyonight_night.conf` reads as
 *  `Tokyonight Night`, and a name with capitals keeps them. */
export function themeLabelFromFileName(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? '';
  const stem = base.replace(/\.(itermcolors|conf|toml|ya?ml|txt|theme)$/i, '');
  const words = stem.replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim();
  if (words === '') return 'Imported theme';
  return words === words.toLowerCase() ? words.replace(/(^|\s)\S/g, (c) => c.toUpperCase()) : words;
}

/** A theme id from a label: lowercase words joined by hyphens. */
export function themeIdFromLabel(label: string): string {
  const slug = label
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return slug === '' ? 'imported' : slug;
}

/** `base` itself when free, else the first free `base-2`, `base-3`, ... */
export function uniqueThemeId(base: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  if (!used.has(base)) return base;
  let n = 2;
  while (used.has(`${base}-${n}`)) n += 1;
  return `${base}-${n}`;
}

/** Read a theme file from Ghostty, iTerm2, Kitty, or Alacritty into a
 *  CustomTheme. `name` is the file name, which picks the label and helps
 *  detection. Pass the ids already in use (built in and custom) to keep
 *  the new id clear of them. Throws a ThemeFileError with a message
 *  ready to show when the file is not a theme Vosh can use. */
export function parseThemeFile(
  name: string,
  text: string,
  takenIds: Iterable<string> = [],
): CustomTheme {
  const format = detectThemeFormat(name, text);
  if (!format) throw new ThemeFileError(UNKNOWN_FORMAT_MESSAGE);
  const slots = readSlots(format, text);
  const { background, foreground } = slots.surfaces;
  const ansi = slots.ansi.filter((c): c is string => c !== undefined);
  if (!background || !foreground || ansi.length !== 16) {
    throw new ThemeFileError(MISSING_COLORS_MESSAGE);
  }
  const s = slots.surfaces;
  const xterm: Record<string, string> = {
    background,
    foreground,
    cursor: s.cursor ?? foreground,
    cursorAccent: s.cursorAccent ?? background,
    // Without a selection color the bright black slot stands in, the
    // shade most palettes use for it.
    selectionBackground: s.selectionBackground ?? ansi[8],
    selectionForeground: s.selectionForeground ?? foreground,
  };
  ANSI_KEYS.forEach((key, i) => {
    xterm[key] = ansi[i];
  });
  const label = slots.name?.trim() || themeLabelFromFileName(name);
  return {
    id: uniqueThemeId(themeIdFromLabel(label), takenIds),
    label,
    description: `Imported from ${FORMAT_LABELS[format]}.`,
    xterm,
    chrome: {},
  };
}
