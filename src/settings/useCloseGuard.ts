import { useEffect, useRef } from 'react';
import { setCloseGuard } from '../lib/pendingWrites';

/** Ask before the window closes while `active` is true. `ask` gets a
 *  `proceed` callback that closes the window after all. The Settings
 *  window's one close handler (useSettingsClose) sends the pending
 *  writes, then runs this guard while one is set. */
export function useCloseGuard(active: boolean, ask: (proceed: () => void) => void): void {
  const askRef = useRef(ask);
  useEffect(() => {
    askRef.current = ask;
  }, [ask]);

  useEffect(() => {
    if (!active) return;
    return setCloseGuard((proceed) => askRef.current(proceed));
  }, [active]);
}
