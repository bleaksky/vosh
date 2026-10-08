import { useEffect, useState } from 'react';
import { isMacPlatform } from '../lib/shortcuts';

// Whether you hold the key Mod means, ⌘ on macOS and Ctrl elsewhere,
// which numbers the sessions sidebar's rows. It shows once the
// key has been held alone for a moment, so a quick shortcut such as ⌘C
// never flashes the numbers, and it stays until you let go, so you can
// read the next number while you press one.

/** How long Mod stays down alone before the numbers show, in ms. */
export const MOD_HOLD_MS = 300;

export function useModHeld(): boolean {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    const mod = isMacPlatform() ? 'Meta' : 'Control';
    let timer: number | undefined;
    let shown = false;
    const wait = () => {
      window.clearTimeout(timer);
      timer = undefined;
    };
    const stop = () => {
      wait();
      shown = false;
      setHeld(false);
    };
    const onDown = (e: KeyboardEvent) => {
      if (shown) return;
      const alone =
        e.key === mod && !e.shiftKey && !e.altKey && (mod === 'Meta' ? !e.ctrlKey : !e.metaKey);
      if (!alone) {
        wait();
        return;
      }
      timer ??= window.setTimeout(() => {
        timer = undefined;
        shown = true;
        setHeld(true);
      }, MOD_HOLD_MS);
    };
    const onUp = (e: KeyboardEvent) => {
      if (e.key === mod) stop();
    };
    window.addEventListener('keydown', onDown, true);
    window.addEventListener('keyup', onUp, true);
    window.addEventListener('blur', stop);
    return () => {
      wait();
      window.removeEventListener('keydown', onDown, true);
      window.removeEventListener('keyup', onUp, true);
      window.removeEventListener('blur', stop);
    };
  }, []);
  return held;
}
