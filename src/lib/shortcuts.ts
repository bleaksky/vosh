// Shortcut labels and the platform they read. One helper turns a
// shortcut spec like 'Mod+Shift+L' into the glyphs a keycap shows, so
// the palette, the menus and help never hard-code ⌘ where Windows and
// Linux use Ctrl. Mod is ⌘ on macOS and Ctrl elsewhere. The specs are
// labels only. The window binds the keys.

/** True on macOS. main.tsx tags the root with data-platform before
 *  the first render, and the user agent covers anything earlier. */
export function isMacPlatform(): boolean {
  if (typeof document !== 'undefined') {
    const tag = document.documentElement.dataset.platform;
    if (tag) return tag === 'macos';
  }
  return typeof navigator !== 'undefined' && /Mac/.test(navigator.userAgent);
}

type Modifier = 'ctrl' | 'alt' | 'shift' | 'mod';

// Apple lists modifiers Control, Option, Shift, Command. Windows and
// Linux write Ctrl, Alt, Shift. On those, Mod is Ctrl.
const MAC_MODS: [Modifier, string][] = [
  ['ctrl', '⌃'],
  ['alt', '⌥'],
  ['shift', '⇧'],
  ['mod', '⌘'],
];
const PC_MODS: [Modifier, string][] = [
  ['ctrl', 'Ctrl'],
  ['alt', 'Alt'],
  ['shift', 'Shift'],
];

const NAMED_KEYS: Record<string, [mac: string, pc: string]> = {
  enter: ['↩', 'Enter'],
  return: ['↩', 'Enter'],
  escape: ['Esc', 'Esc'],
  esc: ['Esc', 'Esc'],
  tab: ['⇥', 'Tab'],
  backspace: ['⌫', 'Backspace'],
  delete: ['⌦', 'Delete'],
  space: ['Space', 'Space'],
  up: ['↑', '↑'],
  down: ['↓', '↓'],
  left: ['←', '←'],
  right: ['→', '→'],
  pageup: ['PgUp', 'PgUp'],
  pagedown: ['PgDn', 'PgDn'],
  home: ['Home', 'Home'],
  end: ['End', 'End'],
};

function parseSpec(spec: string): { mods: Set<Modifier>; key: string } {
  // A trailing '++' means the key itself is '+'.
  const plusKey = spec.endsWith('++');
  const parts = (plusKey ? spec.slice(0, -2) : spec).split('+').filter((p) => p.length > 0);
  const key = plusKey ? '+' : (parts.pop() ?? '');
  const mods = new Set<Modifier>();
  for (const raw of parts) {
    const m = raw.toLowerCase();
    if (m === 'mod' || m === 'cmd' || m === 'meta') mods.add('mod');
    else if (m === 'ctrl' || m === 'control') mods.add('ctrl');
    else if (m === 'alt' || m === 'option' || m === 'opt') mods.add('alt');
    else if (m === 'shift') mods.add('shift');
  }
  return { mods, key };
}

/** The keycaps for a shortcut spec, modifiers first in the platform's
 *  order: `shortcutKeys('Mod+Shift+L')` is ⇧ ⌘ L on macOS and
 *  Ctrl Shift L elsewhere. */
export function shortcutKeys(spec: string, mac: boolean = isMacPlatform()): string[] {
  const { mods, key } = parseSpec(spec);
  const out: string[] = [];
  if (mac) {
    for (const [m, glyph] of MAC_MODS) if (mods.has(m)) out.push(glyph);
  } else {
    // Mod is Ctrl here, so Mod+Ctrl collapses to one Ctrl.
    if (mods.has('mod')) mods.add('ctrl');
    for (const [m, name] of PC_MODS) if (mods.has(m)) out.push(name);
  }
  const named = NAMED_KEYS[key.toLowerCase()];
  if (named) out.push(mac ? named[0] : named[1]);
  else if (key.length === 1) out.push(key.toUpperCase());
  else if (key.length > 1) out.push(key[0].toUpperCase() + key.slice(1));
  return out;
}

/** A shortcut as one inline label for menus: ⌘C on macOS, Ctrl+C
 *  elsewhere. */
export function shortcutLabel(spec: string, mac: boolean = isMacPlatform()): string {
  return shortcutKeys(spec, mac).join(mac ? '' : '+');
}

// The KeyboardEvent.key names aria-keyshortcuts expects for the named
// keys. Any other key longer than one letter, like F2, is already one.
const ARIA_KEYS: Record<string, string> = {
  enter: 'Enter',
  return: 'Enter',
  escape: 'Escape',
  esc: 'Escape',
  tab: 'Tab',
  backspace: 'Backspace',
  delete: 'Delete',
  space: 'Space',
  up: 'ArrowUp',
  down: 'ArrowDown',
  left: 'ArrowLeft',
  right: 'ArrowRight',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  home: 'Home',
  end: 'End',
};

/** A shortcut spec as aria-keyshortcuts reads it, so a screen reader
 *  names the keys apart from the control: `ariaKeyshortcuts('Mod+K')`
 *  is Meta+K on macOS and Control+K elsewhere. Modifiers come in the
 *  order Control, Meta, Alt, Shift. */
export function ariaKeyshortcuts(spec: string, mac: boolean = isMacPlatform()): string {
  const { mods, key } = parseSpec(spec);
  const out: string[] = [];
  if (mods.has('ctrl') || (!mac && mods.has('mod'))) out.push('Control');
  if (mac && mods.has('mod')) out.push('Meta');
  if (mods.has('alt')) out.push('Alt');
  if (mods.has('shift')) out.push('Shift');
  const named = ARIA_KEYS[key.toLowerCase()];
  if (named) out.push(named);
  else if (key.length === 1) out.push(key.toUpperCase());
  else if (key.length > 1) out.push(key[0].toUpperCase() + key.slice(1));
  return out.join('+');
}

// Physical keys the window shortcuts use, for layouts whose keys type
// something else.
const CODE_KEYS: Record<string, string> = {
  Comma: ',',
  Slash: '/',
  Backslash: '\\',
};

/** The key a window shortcut matches on, lowercased. Latin layouts
 *  (Dvorak and AZERTY included) match on the character the key types.
 *  A non-Latin layout (Cyrillic, Greek) types a letter no shortcut
 *  names, so it falls back to the physical key: Ctrl+R there still
 *  reads as `r` and never lets WebView2 reload the page. */
export function shortcutKey(e: { key: string; code: string }): string {
  const key = e.key.toLowerCase();
  if (key.length !== 1 || (key >= ' ' && key <= '~')) return key;
  const letter = /^Key([A-Z])$/.exec(e.code);
  if (letter) return letter[1].toLowerCase();
  return CODE_KEYS[e.code] ?? key;
}
