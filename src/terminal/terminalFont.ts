// xterm measures its cell when it opens and each time its font option
// changes to a new value. A face of the list that is still loading then
// measures as the next family in the list that has loaded, and xterm
// keeps that cell until the pane changes size. The page mints the faces
// a saved list names only after the terminal takes the list, so on a
// first launch an installed font you picked measured as the bundled
// JetBrains Mono and the rows sat wider apart than on every later launch.
// WebKit fires no loadingdone for a face the page loads on its own, so
// the terminal asks for the face itself and measures once it has loaded.

/** The part of an xterm the remeasure reads and sets. */
export interface FontOptions {
  options: { fontFamily?: string; fontSize?: number };
}

/** The part of a FontFaceSet the remeasure loads faces through. */
export interface FaceLoader {
  load(font: string): Promise<unknown>;
}

/** Make xterm measure its cell again. xterm ignores a set to the value it
 *  holds, so this sets the list with a trailing space, which names the
 *  same faces, and then the list itself. */
export function remeasureCell(term: FontOptions): void {
  const family = term.options.fontFamily ?? '';
  term.options.fontFamily = `${family} `;
  term.options.fontFamily = family;
}

/** The family names in a font list, unquoted. */
function familyNames(list: string): string[] {
  return list
    .split(',')
    .map((piece) =>
      piece
        .trim()
        .replace(/^["']|["']$/g, '')
        .trim(),
    )
    .filter(Boolean);
}

/** Load the face `list` draws with at `px` and settle once it has. That is
 *  the face of the first family that loads. A face that fails to load
 *  falls back to the next family, as the browser does. A family with no
 *  face to load, such as a generic, ends the walk, since the browser has
 *  it at once. */
export async function loadDrawnFace(fonts: FaceLoader, px: number, list: string): Promise<void> {
  for (const name of familyNames(list)) {
    const css = /^[a-z-]+$/i.test(name) ? name : JSON.stringify(name);
    try {
      await fonts.load(`${px}px ${css}`);
      return;
    } catch {
      // The face failed to load, so the next family draws.
    }
  }
}

/** Measure the cell of `term` again once the face its list draws with has
 *  loaded, then call `after`, which fits the pane to the cell. It waits a
 *  microtask first, so a face an outer effect of the same commit mints
 *  counts. Returns the cancel, which drops a remeasure the next font or
 *  an unmount makes stale. */
export function remeasureWhenLoaded(
  fonts: FaceLoader,
  term: FontOptions,
  after: () => void,
): () => void {
  let live = true;
  const list = term.options.fontFamily ?? '';
  const px = term.options.fontSize ?? 14;
  void Promise.resolve()
    .then(() => (live ? loadDrawnFace(fonts, px, list) : undefined))
    .then(() => {
      if (!live) return;
      remeasureCell(term);
      after();
    });
  return () => {
    live = false;
  };
}
