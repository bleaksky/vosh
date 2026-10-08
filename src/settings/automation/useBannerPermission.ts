import { useEffect, useState } from 'react';
import { alertsAskPermission, alertsPermission, type Permission } from '../../ipc/alerts';

// Whether the system lets Vosh post banners, and the ask Vosh shows
// before the system's own question.

/** Not now holds for the life of this Settings window, so a second
 *  Banner press asks no more. */
let notNow = false;

export interface BannerPermission {
  /** What the system says, or null until the first read lands. */
  permission: Permission | null;
  /** Vosh's ask is open. */
  asking: boolean;
  /** Run `then`, after the ask while the system has not asked yet and
   *  you have not pressed Not now in this window. */
  askFirst: (then: () => void) => void;
  /** Close the ask. Either way `then` runs, so Not now leaves the Banner
   *  part on. Continue asks the system and keeps its answer. */
  answer: (go: boolean) => void;
}

/** Reads `alerts_permission` on mount and again each time the window
 *  comes back to the front, as it does after a trip to System Settings.
 *  A read that fails keeps the last answer. */
export function useBannerPermission(): BannerPermission {
  const [permission, setPermission] = useState<Permission | null>(null);
  const [pending, setPending] = useState<{ run: () => void } | null>(null);

  useEffect(() => {
    let live = true;
    const read = () => {
      alertsPermission().then(
        (p) => {
          if (live) setPermission(p);
        },
        () => {},
      );
    };
    read();
    window.addEventListener('focus', read);
    return () => {
      live = false;
      window.removeEventListener('focus', read);
    };
  }, []);

  const askFirst = (then: () => void) => {
    if (permission === 'not_asked' && !notNow) setPending({ run: then });
    else then();
  };

  const answer = (go: boolean) => {
    setPending(null);
    pending?.run();
    if (!go) {
      notNow = true;
      return;
    }
    alertsAskPermission().then(setPermission, () => {});
  };

  return { permission, asking: pending !== null, askFirst, answer };
}
