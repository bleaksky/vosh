// Lists class selectors in src/styles that no source file uses, and exits 1
// when any are left. Run it with `npm run css:usage`.
//
// A class counts as used when one of these holds.
// 1. A word token in src/**/*.{ts,tsx} (tests left out) or index.html spells it.
// 2. The code builds it at run time. That means it starts with a prefix the
//    code builds, the text before `${` in a template literal or a string
//    literal followed by `+`, and the rest of the name is a string literal in
//    the source, such as the 'danger' in `is-${tone}`. A prefix alone is not
//    enough, so `pane-${leaf.pane}` vouches for .pane-map but not .pane-zzz.
//    Literals given to key, id, htmlFor or an aria attribute build no class
//    and add no prefix.
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
const literals = new Set();
const prefixes = new Set();
// The text just before a literal that names a key, an id or an aria
// attribute rather than a class.
const NOT_A_CLASS = /(?:\bkey|\bid|\bhtmlFor|\baria-[\w-]+)\s*[=:]\s*\{?\s*$/;
const contextOf = (text, quote) => text.slice(Math.max(0, quote - 40), quote);
for (const text of sources) {
  for (const [token] of text.matchAll(/[\w-]+/g)) tokens.add(token);
  for (const match of text.matchAll(/'([\w-]+)'|"([\w-]+)"|`([\w-]+)`/g)) {
    const word = match[1] ?? match[2] ?? match[3];
    literals.add(word).add(word.replace(/_/g, '-'));
  }
  const built = [
    ...[...text.matchAll(/([\w-]*)\$\{/g)].map((m) => [m[1], text.lastIndexOf('`', m.index)]),
    ...[...text.matchAll(/['"`]([\w-]*)['"`]\s*\+/g)].map((m) => [m[1], m.index]),
  ];
  // A prefix must start with a letter and end with a dash. That keeps loose
  // fragments such as the lone dash in `--${name}` from passing every class.
  for (const [prefix, quote] of built) {
    if (!/^[A-Za-z][\w-]*-$/.test(prefix)) continue;
    if (NOT_A_CLASS.test(contextOf(text, quote))) continue;
    prefixes.add(prefix);
  }
}

const isBuilt = (name) =>
  THIRD_PARTY.some((prefix) => name.startsWith(prefix)) ||
  [...prefixes].some(
    (prefix) => name.startsWith(prefix) && literals.has(name.slice(prefix.length)),
  );

const unused = [];
for (const file of readdirSync(stylesDir)
  .filter((name) => name.endsWith('.css'))
  .sort()) {
  for (const name of classesIn(readFileSync(join(stylesDir, file), 'utf8'))) {
    const used = tokens.has(name) || isBuilt(name);
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
