// The prompt card saves for one profile, the active one. When another
// profile becomes active, by a switch, the auto switch when you log in
// as a character it claims, or a config that replaces the live one
// (#profile load or reset, an import), the card opens again for it, so
// it never saves one profile's table over another's. A login names the
// character the header saves for.

import { subscribeSessionIdentity, type SessionIdentity } from '../ipc/characters';
import { subscribeProfileSwitched } from '../ipc/profiles';
import { subscribeUiConfigReplaced } from '../ipc/uiConfig';

export interface CardProfileFollow {
  /** Another profile's table is live now. */
  reopen: () => void;
  /** The session identity changed, so the header may name another
   *  character. */
  identity: (who: SessionIdentity | null) => void;
}

/** Hear what changes the profile the card saves for. Resolves to a
 *  function that stops listening. */
export async function followCardProfile(follow: CardProfileFollow): Promise<() => void> {
  const stops = await Promise.all([
    subscribeUiConfigReplaced(() => follow.reopen()),
    subscribeProfileSwitched(() => follow.reopen()),
    subscribeSessionIdentity((who) => follow.identity(who)),
  ]);
  return () => {
    for (const stop of stops) stop();
  };
}
