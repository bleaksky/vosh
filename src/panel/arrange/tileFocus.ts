// Where the keys in arrange mode are described, and where the caret
// goes after a key moves a tile.

/** The line that says what the keys do, which every tile points at. */
export const ARRANGE_KEYS_ID = 'arrange-keys';

/** Focus the tile or shelf tile of leaf `id`, after a key moved it. */
export function focusTile(id: string): void {
  const tile = document.querySelector<HTMLElement>(
    `[data-arrange-tile="${CSS.escape(id)}"], [data-shelf-tile="${CSS.escape(id)}"]`,
  );
  tile?.focus({ preventScroll: true });
}
