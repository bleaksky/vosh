// Lists class selectors in src/styles that no source file uses, and exits 1
// when any are left. Run it with `npm run css:usage`.
//
// A class counts as used when one of these holds.
// 1. A word token in src/**/*.{ts,tsx} (tests left out) or index.html spells it.
// 2. It starts with a prefix the code builds at run time, the text before
//    `${` in a template literal or a string literal followed by `+`.
// 3. It starts with a prefix in THIRD_PARTY, for classes a library writes.

import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const stylesDir = join(root, 'src', 'styles');

const THIRD_PARTY = [
  'xterm-', // xterm.js
  'cm-', // CodeMirror
  'tok-', // @lezer/highlight classHighlighter, used by src/help/helpCode.ts
];

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

// Class names from the selector preludes of one stylesheet. Declarations are
// skipped, so numbers like 1.5em never read as classes.
function classesIn(css) {
  const text = css
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/url\([^)]*\)/g, 'url()')
    .replace(/"[^"]*"|'[^']*'/g, '""');
  const names = new Set();
  for (const [, prelude] of text.matchAll(/([^{};]*)\{/g)) {
    for (const [, name] of prelude.matchAll(/\.(-?[A-Za-z_][\w-]*)/g)) names.add(name);
  }
  return names;
}

const isSource = (path) =>
  /\.(ts|tsx)$/.test(path) &&
  !/\.test\.tsx?$/.test(path) &&
  !relative(root, path).startsWith(join('src', 'test'));

const sources = [...walk(join(root, 'src')).filter(isSource), join(root, 'index.html')].map(
  (path) => readFileSync(path, 'utf8'),
);

const tokens = new Set();
const prefixes = new Set(THIRD_PARTY);
for (const text of sources) {
  for (const [token] of text.matchAll(/[\w-]+/g)) tokens.add(token);
  const built = [...text.matchAll(/([\w-]*)\$\{/g), ...text.matchAll(/['"`]([\w-]*)['"`]\s*\+/g)];
  // A prefix must start with a letter and end with a dash. That keeps loose
  // fragments such as the lone dash in `--${name}` from passing every class.
  for (const [, prefix] of built) {
    if (/^[A-Za-z][\w-]*-$/.test(prefix)) prefixes.add(prefix);
  }
}

const unused = [];
for (const file of readdirSync(stylesDir)
  .filter((name) => name.endsWith('.css'))
  .sort()) {
  for (const name of classesIn(readFileSync(join(stylesDir, file), 'utf8'))) {
    const used = tokens.has(name) || [...prefixes].some((prefix) => name.startsWith(prefix));
    if (!used) unused.push(`src/styles/${file}  .${name}`);
  }
}

if (unused.length > 0) {
  console.error(
    `Nothing uses ${unused.length === 1 ? 'this CSS class' : `these ${unused.length} CSS classes`}.`,
  );
  for (const line of unused) console.error(`  ${line}`);
  process.exit(1);
}
console.log('Every CSS class in src/styles is used.');
