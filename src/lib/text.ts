// The wording Vosh builds from names and lists, so every sentence that
// joins them reads the same way.

/** Items as a sentence lists them: `Orla`, `Maren and Orla`, or
 *  `Tolliver, Maren, and Orla` with a serial comma before the last of
 *  three or more. */
export function listJoin(items: readonly string[]): string {
  if (items.length <= 2) return items.join(' and ');
  return `${items.slice(0, -1).join(', ')}, and ${items[items.length - 1]}`;
}
