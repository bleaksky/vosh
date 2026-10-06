import { describe, expect, it, vi } from 'vitest';
import { pushToast } from '../stores/toasts';
import { gamePromptToast, lostPartsToast, startGamePromptToasts } from './gamePromptToast';

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => [] }));
vi.mock('../stores/toasts', () => ({ pushToast: vi.fn() }));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

describe('gamePromptToast', () => {
  it('names the codes your capture took', () => {
    expect(gamePromptToast({ kind: 'gmcp', text: '%h %m ', applied: true, lost: [] })).toEqual({
      kind: 'info',
      message: 'Vosh reads your new prompt.',
      meta: '%h %m ',
      metaMono: true,
    });
    expect(
      gamePromptToast({ kind: 'prompt', text: '<%hhp> ', applied: true, lost: [] })?.meta,
    ).toBe('<%hhp> ');
  });

  it('stays quiet when the capture took nothing', () => {
    expect(gamePromptToast({ kind: 'gmcp', text: '%h ', applied: false, lost: [] })).toBeNull();
    expect(gamePromptToast({ kind: 'off', text: '', applied: true, lost: [] })).toBeNull();
  });
});

describe('lostPartsToast', () => {
  it('says once which part of your design nothing feeds now (P14)', () => {
    expect(
      lostPartsToast({ kind: 'prompt', text: '%n%C[%h/%Hhp]%c', applied: true, lost: ['tank_hp'] }),
    ).toEqual({
      kind: 'info',
      message:
        "Your prompt no longer shows your tank's health, so that part of your design stays blank.",
    });
  });

  it('names every part, and says those parts for more than one', () => {
    expect(
      lostPartsToast({
        kind: 'gmcp',
        text: '[%h]',
        applied: true,
        lost: ['pos', 'slot1', 'stallion'],
      })?.message,
    ).toBe(
      'Your prompt no longer shows your position, affect slot 1, and your stallion, so those parts of your design stay blank.',
    );
  });

  it('stays quiet when every part is still fed or the capture took nothing', () => {
    expect(lostPartsToast({ kind: 'gmcp', text: '[%h]', applied: true, lost: [] })).toBeNull();
    expect(
      lostPartsToast({ kind: 'gmcp', text: '[%h]', applied: false, lost: ['tank_hp'] }),
    ).toBeNull();
  });
});

describe('the toasts for a new prompt', () => {
  it('speak only for the session in front', async () => {
    const stop = await startGamePromptToasts();
    const seen = { kind: 'gmcp', text: '%h %m ', applied: true, lost: [] };
    // Before the list comes, the first session is in front.
    fire('session://game-prompt-seen', { session: 2, ...seen });
    expect(pushToast).not.toHaveBeenCalled();
    fire('session://game-prompt-seen', { session: 1, ...seen });
    expect(pushToast).toHaveBeenCalledTimes(1);
    stop();
  });
});
