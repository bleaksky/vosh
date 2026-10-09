import { useCallback } from 'react';
import { setWritingSlot } from './pinnedPane';

// The Writing pane in the panel. It draws only the slot the pinned
// writing card fills, header and all, so the card keeps its own header,
// with the pin and Close, in place of a pane header.

export function WritingPane() {
  const ref = useCallback((el: HTMLDivElement | null) => setWritingSlot(el), []);
  return <div ref={ref} className="wr-pane-slot" />;
}
