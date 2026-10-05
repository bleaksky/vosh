// Dynamic @font-face injection for system fonts that WKWebView refuses
// to match by name. The backend font scheme serves the regular face of
// a family named in the URL path. We mint an @font-face block here
// pointing at that URL so CSS font-family resolves to the served bytes.

import { convertFileSrc } from '@tauri-apps/api/core';

const STYLE_ID = 'vosh-dynamic-font-face';
const loaded = new Set<string>();

function styleEl(): HTMLStyleElement {
  let el = document.getElementById(STYLE_ID) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement('style');
    el.id = STYLE_ID;
    document.head.appendChild(el);
  }
  return el;
}

// The URL the backend font scheme serves `family` at. Tauri's
// convertFileSrc encodes the family into the path, which is where the
// backend reads it: font://localhost/<family> on macOS and Linux,
// http://font.localhost/<family> on Windows. Null outside Tauri, where
// no scheme answers.
export function fontUrl(family: string): string | null {
  try {
    return convertFileSrc(family, 'font');
  } catch {
    return null;
  }
}

// The @font-face block that names the face at `url` after `family`.
export function fontFaceCss(family: string, url: string): string {
  return `
@font-face {
  font-family: ${JSON.stringify(family)};
  font-style: normal;
  font-weight: 400;
  font-display: block;
  src: url(${JSON.stringify(url)});
}
`;
}

// Inject (or no-op if already injected) an @font-face for the given
// family. The CSS family name in the @font-face matches `family` so
// downstream font-family rules can target it directly.
export function loadSystemFont(family: string): void {
  if (!family) return;
  if (loaded.has(family)) return;
  const url = fontUrl(family);
  if (!url) return;
  loaded.add(family);
  styleEl().appendChild(document.createTextNode(fontFaceCss(family, url)));
}

// Walk a font-family CSS value, pick out quoted or unquoted family
// names, and request each one. Anything that looks like a generic
// (monospace, sans-serif, ...) or a CSS keyword is skipped.
const GENERIC_FAMILIES = new Set([
  'monospace',
  'serif',
  'sans-serif',
  'cursive',
  'fantasy',
  'system-ui',
  'ui-monospace',
  'ui-serif',
  'ui-sans-serif',
  'ui-rounded',
  'inherit',
  'initial',
  'unset',
]);

// The family the bundled JetBrains Mono goes by in fonts.css.
const BUNDLED_FAMILY = 'JetBrainsMono Bundled';

// The family Berkeley Mono went by while Vosh bundled it, which saved
// font lists still name, and the installed families that stand in for
// it: the Nerd Font build Vosh bundled, then the family the foundry
// sells.
const RETIRED_BERKELEY = 'berkeleymono bundled';
const BERKELEY_FAMILIES = ['BerkeleyMono Nerd Font', 'Berkeley Mono'];

function unquote(piece: string): string {
  return piece
    .trim()
    .replace(/^["']|["']$/g, '')
    .trim();
}

function cssFamily(name: string): string {
  return GENERIC_FAMILIES.has(name.toLowerCase()) ? name : JSON.stringify(name);
}

// The font list the page renders for a saved one. Vosh no longer ships
// Berkeley Mono, so a Berkeley name stands for your installed copy, and
// the bundled JetBrains Mono follows each run of Berkeley names for a
// machine without one. The retired bundled name becomes the installed
// Berkeley families. A repeated name drops out. A list without Berkeley
// Mono comes back as it is. rendered_families in
// src-tauri/src/native/gpu/atlas.rs gives the native atlas the same list,
// so both renderers land on the same face and cell. Both run
// fixtures/font-stacks/cases.json.
export function renderFontStack(stack: string): string {
  if (!/berkeley/i.test(stack)) return stack;
  const out: string[] = [];
  const push = (name: string) => {
    if (!out.some((f) => f.toLowerCase() === name.toLowerCase())) out.push(name);
  };
  let afterBerkeley = false;
  for (const piece of stack.split(',')) {
    const name = unquote(piece);
    if (!name) continue;
    const berkeley = /berkeley/i.test(name);
    if (afterBerkeley && !berkeley) push(BUNDLED_FAMILY);
    afterBerkeley = berkeley;
    if (name.toLowerCase() === RETIRED_BERKELEY) BERKELEY_FAMILIES.forEach(push);
    else push(name);
  }
  if (afterBerkeley) push(BUNDLED_FAMILY);
  return out.map(cssFamily).join(', ');
}

export function loadFontStack(stack: string): void {
  if (!stack) return;
  for (const piece of stack.split(',')) {
    const name = piece.trim().replace(/^["']|["']$/g, '');
    if (!name) continue;
    if (GENERIC_FAMILIES.has(name.toLowerCase())) continue;
    // Bundled families ship via static @font-face in fonts.css and
    // are not resolvable through the system enumeration.
    if (name.endsWith('Bundled')) continue;
    loadSystemFont(name);
  }
}
