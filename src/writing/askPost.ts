import { useEffect, useState } from 'react';
import { getUiConfig, subscribeWritingAskPostChanged } from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';

// Settings › Input › Ask before you post, and the card's own Don't ask
// again under Post…'s confirm, which turns it off.

/** Whether the card asks before it posts, live as Settings changes it. */
export function useAskPost(): boolean {
  const [on, setOn] = useState(true);
  useEffect(() => {
    let live = true;
    getUiConfig()
      .then((config) => live && setOn(config.writing_ask_post))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  useTauriEvent(subscribeWritingAskPostChanged, (next) => setOn(Boolean(next)));
  return on;
}

/** Whether a confirm you took turns Ask before you post off: it offered
 *  Don't ask again and you turned it on. */
export function stopsAsking(offered: boolean | undefined, on: boolean): boolean {
  return offered === true && on;
}
