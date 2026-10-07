import { useState, type MouseEvent, type ReactNode } from 'react';
import { VitalsMenu } from './VitalsMenu';

/** The vitals menu for a place that draws your vitals: `open` takes the
 *  right click, and `menu` goes in the tree. */
export function useVitalsMenu(): { open: (e: MouseEvent) => void; menu: ReactNode } {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return {
    open: (e) => {
      // In place of the web view's own menu.
      e.preventDefault();
      setAt({ x: e.clientX, y: e.clientY });
    },
    menu: at && <VitalsMenu x={at.x} y={at.y} onClose={() => setAt(null)} />,
  };
}
