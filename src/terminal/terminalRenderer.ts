/** Session flag set when the native surface never came up, so the page
 *  falls back to xterm instead of leaving a transparent hole. */
export const NATIVE_FAILED_KEY = 'vosh.nativesurface.failed';

// The native wgpu terminal surface. It is on by default on macOS, where
// it draws what xterm draws (docs/native-renderer.md). There
// vosh.nativesurface '0' falls back to xterm. Windows and Linux always
// draw with xterm and never read the flag. Vosh keeps no native surface
// for them, and in native mode the page waits for the surface to size
// the grid, so a forced flag there would leave the terminal without a
// size.
//
// The surface sits BELOW the webview (the underlay). The page leaves the
// terminal pane unpainted so the grid shows through, DOM overlays draw
// over it with no renderer swap, and pointer input over the pane is
// forwarded to the surface.
export function nativeSurfaceEnabled(): boolean {
  if (typeof localStorage === 'undefined') return false;
  // The surface failed to come up earlier in this session. Stay on xterm
  // until the next launch.
  try {
    if (sessionStorage.getItem(NATIVE_FAILED_KEY) === '1') return false;
  } catch {
    // storage unavailable; fall through to the platform
  }
  const mac =
    typeof navigator !== 'undefined' &&
    (navigator.platform.startsWith('Mac') || navigator.userAgent.includes('Mac OS'));
  if (!mac) return false;
  // '1' and no flag both leave the macOS default on.
  return localStorage.getItem('vosh.nativesurface') !== '0';
}
