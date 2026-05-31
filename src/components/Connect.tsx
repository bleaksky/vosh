import { useState, type FormEvent } from 'react';
import {
  connectSession,
  disconnectSession,
  profileResolveMatch,
  profileSwitch,
  profilesList,
} from '../lib/session';

interface Props {
  status: ConnectionStatus;
  onError: (message: string) => void;
}

export type ConnectionStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'error'; message: string };

const DEFAULT_HOST = 'play.theforsakenlands.com';
const DEFAULT_PORT = 1848;
// Remember the last character typed per host so the user does not
// have to re-enter it on every reconnect. Stored under a host-scoped
// key so different MUDs do not stomp on each other.
const CHAR_KEY_PREFIX = 'vosh.connect.character.';
const charStorageKey = (host: string) => `${CHAR_KEY_PREFIX}${host.trim().toLowerCase()}`;

function loadCharacterFor(host: string): string {
  try {
    return localStorage.getItem(charStorageKey(host)) ?? '';
  } catch {
    return '';
  }
}

function saveCharacterFor(host: string, name: string) {
  try {
    const key = charStorageKey(host);
    if (name.trim().length > 0) {
      localStorage.setItem(key, name.trim());
    } else {
      localStorage.removeItem(key);
    }
  } catch {
    // localStorage may be unavailable in some sandboxes; tolerate.
  }
}

export function Connect({ status, onError }: Props) {
  const [host, setHost] = useState(DEFAULT_HOST);
  const [port, setPort] = useState(DEFAULT_PORT);
  const [tls, setTls] = useState(false);
  const [character, setCharacter] = useState(() => loadCharacterFor(DEFAULT_HOST));
  const isLive = status.kind === 'connecting' || status.kind === 'connected';

  // When the user retypes the host, swap to the remembered
  // character for that host so character autoselect tracks the
  // server they are about to dial.
  const handleHostChange = (next: string) => {
    setHost(next);
    setCharacter(loadCharacterFor(next));
  };

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      if (isLive) {
        await disconnectSession();
        return;
      }
      // Auto-switch to a matching profile before opening the
      // connection. Match is by host (required) + port (when
      // pinned by the profile) + character (when pinned). Falling
      // back to the currently-active profile when nothing matches
      // is the natural behavior: existing connections keep using
      // whatever profile is loaded today.
      const characterArg = character.trim().length > 0 ? character.trim() : null;
      if (characterArg) saveCharacterFor(host, characterArg);
      try {
        const matchName = await profileResolveMatch(host, port, characterArg);
        if (matchName) {
          const current = await profilesList();
          if (matchName !== current.active) {
            await profileSwitch(matchName);
          }
        }
      } catch (matchErr) {
        // Profile resolve is best-effort — never block a connect on
        // a transient profile-system error.
        console.warn('[profile match]', matchErr);
      }
      await connectSession(host, port, tls);
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <form className={`connect${isLive ? ' is-live' : ''}`} onSubmit={handleSubmit}>
      <span className="connect-label">host:</span>
      <input
        type="text"
        value={host}
        disabled={isLive}
        spellCheck={false}
        onChange={(e) => handleHostChange(e.target.value)}
        aria-label="host"
      />
      <span className="connect-label">port:</span>
      <input
        type="number"
        value={port}
        disabled={isLive}
        min={1}
        max={65535}
        onChange={(e) => setPort(Number(e.target.value))}
        aria-label="port"
      />
      <span
        className="connect-label"
        title="optional — pin a profile to this character at connect time"
      >
        char:
      </span>
      <input
        type="text"
        value={character}
        disabled={isLive}
        spellCheck={false}
        placeholder="optional"
        onChange={(e) => setCharacter(e.target.value)}
        aria-label="character"
      />
      <label className="connect-tls">
        <span className="connect-label">tls:</span>
        <input
          type="checkbox"
          checked={tls}
          disabled={isLive}
          onChange={(e) => setTls(e.target.checked)}
        />
      </label>
      <button type="submit" className="connect-action">
        {isLive ? '[disconnect]' : '[connect]'}
      </button>
    </form>
  );
}
