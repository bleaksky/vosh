import { describe, expect, it } from 'vitest';
import type { ConnectionStatus } from '../stores/session/connectionStore';
import { windowTitle } from './windowTitle';

const live: ConnectionStatus = {
  kind: 'connected',
  host: 'play.theforsakenlands.com',
  port: 4000,
  tls: false,
};

describe('windowTitle', () => {
  it('names the character and the world while connected', () => {
    expect(windowTitle(live, 'Ilsabet', 'The Forsaken Lands')).toBe(
      'Ilsabet on The Forsaken Lands',
    );
  });

  it('names the world alone before you log in', () => {
    expect(windowTitle(live, null, 'The Forsaken Lands')).toBe('The Forsaken Lands');
  });

  it('reads Vosh when no session is up', () => {
    expect(windowTitle({ kind: 'idle' }, null, 'The Forsaken Lands')).toBe('Vosh');
    expect(windowTitle({ kind: 'error', message: 'refused' }, 'Ilsabet', 'Somewhere')).toBe('Vosh');
    expect(
      windowTitle(
        { kind: 'connecting', host: 'play.theforsakenlands.com', port: 4000, tls: false },
        null,
        'The Forsaken Lands',
      ),
    ).toBe('Vosh');
  });
});
