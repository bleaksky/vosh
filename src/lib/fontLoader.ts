// Dynamic @font-face injection for system fonts that WKWebView refuses
// to match by name. Backend exposes a font:// URI scheme that streams
// font files by family name. We mint an @font-face block here pointing
// at that URI so CSS font-family resolves to the served bytes.

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

// Inject (or no-op if already injected) an @font-face for the given
// family. The CSS family name in the @font-face matches `family` so
// downstream font-family rules can target it directly.
export function loadSystemFont(family: string): void {
  if (!family) return;
  if (loaded.has(family)) return;
  loaded.add(family);
  const encoded = encodeURIComponent(family);
  const css = `
@font-face {
  font-family: ${JSON.stringify(family)};
  font-style: normal;
  font-weight: 400;
  font-display: block;
  src: url("font://${encoded}");
}
`;
  styleEl().appendChild(document.createTextNode(css));
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

export function loadFontStack(stack: string): void {
  if (!stack) return;
  for (const piece of stack.split(',')) {
    const name = piece.trim().replace(/^["']|["']$/g, '');
    if (!name) continue;
    if (GENERIC_FAMILIES.has(name.toLowerCase())) continue;
    // Bundled families ship via static @font-face in styles.css and
    // are not resolvable through the system enumeration.
    if (name.endsWith('Bundled')) continue;
    loadSystemFont(name);
  }
}
