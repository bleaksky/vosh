// The button that shows a file Vosh wrote for you in the file manager,
// through one command of Vosh's own (Q27 of the Scripts review). A
// plugin's folder and a saved scene both use it.

/** The button's words for the platform the page runs on. */
export function revealLabel(platform: string | undefined): string {
  if (platform === 'macos') return 'Show in Finder';
  if (platform === 'windows') return 'Show in Explorer';
  return 'Show the folder';
}
