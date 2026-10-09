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

// The family name of the font Vosh bundled before JetBrains Mono, which
// saved font lists still name. Vosh never ships it again, so the name
// stands for the bundled JetBrains Mono.
const RETIRED_BUNDLED = 'berkeleymono bundled';

// The default font list from those days. A profile that never picked a
// font still holds it, and it draws as the default list does now.
const RETIRED_DEFAULT =
  'berkeleymono nerd font, jetbrains mono, fira code, menlo, consolas, ui-monospace, monospace';
const DEFAULT_STACK = '"JetBrainsMono Bundled", Menlo, Consolas, ui-monospace, monospace';

function unquote(piece: string): string {
  return piece
    .trim()
    .replace(/^["']|["']$/g, '')
    .trim();
}

function cssFamily(name: string): string {
  return GENERIC_FAMILIES.has(name.toLowerCase()) ? name : JSON.stringify(name);
}

// The font list the page renders for a saved one. The retired default
// list becomes the default list, and the retired bundled name becomes
// the bundled JetBrains Mono, with a repeated name dropped. Every other
// list comes back as it is. rendered_families in
// src-tauri/src/native/gpu/atlas.rs gives the native atlas the same list,
// so both renderers land on the same face and cell. Both run
// fixtures/font-stacks/cases.json.
export function renderFontStack(stack: string): string {
  if (stack.trim().toLowerCase() === RETIRED_DEFAULT) return DEFAULT_STACK;
  if (!stack.toLowerCase().includes(RETIRED_BUNDLED)) return stack;
  const out: string[] = [];
  for (const piece of stack.split(',')) {
    const unquoted = unquote(piece);
    if (!unquoted) continue;
    const name = unquoted.toLowerCase() === RETIRED_BUNDLED ? BUNDLED_FAMILY : unquoted;
    if (!out.some((f) => f.toLowerCase() === name.toLowerCase())) out.push(name);
  }
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
