// The wording Vosh builds from names, lists and errors, so every
// sentence that shows them reads the same way.

/** Items as a sentence lists them: `Orla`, `Maren and Orla`, or
 *  `Tolliver, Maren, and Orla` with a serial comma before the last of
 *  three or more. */
export function listJoin(items: readonly string[]): string {
  if (items.length <= 2) return items.join(' and ');
  return `${items.slice(0, -1).join(', ')}, and ${items[items.length - 1]}`;
}

/** A name's possessive, `Orla's`, and `Rhys's` for a name that ends in
 *  s as well, so every name reads the same way. */
export function possessive(name: string): string {
  return `${name}'s`;
}

/** A name you gave something, in curly quotes: `“Orla”`. */
export function quoted(name: string): string {
  return `“${name}”`;
}

/** What went wrong, as the text to show you. Commands reject with a
 *  plain string and the page throws Error, so both read the same. */
export function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).trim();
}
