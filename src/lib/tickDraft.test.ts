import { describe, expect, it, vi } from 'vitest';
import { listen, type EventCallback } from '@tauri-apps/api/event';
import type { TickConfig } from './session';
import { followTickDraft } from './tickDraft';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

const tick = (patch: Partial<TickConfig> = {}): TickConfig => ({
  enabled: true,
  interval_secs: 30,
  auto_fire: null,
  sound: true,
  reset_pattern: null,
  warn_at_secs: 5,
  warn_message: null,
  warn_color: null,
  ...patch,
});

/** Follow the tick the way the Tick card does, with a draft that is
 *  dirty when `dirty` says so, and hand back a way to send an event. */
async function card(dirty: () => boolean) {
  const heard = new Map<string, EventCallback<unknown>>();
  vi.mocked(listen).mockImplementation(((event: string, handler: EventCallback<unknown>) => {
    heard.set(event, handler);
    return Promise.resolve(() => heard.delete(event));
  }) as typeof listen);
  const reload = vi.fn();
  const adopt = vi.fn();
  const stop = await followTickDraft({ reload, adopt, isDirty: dirty });
  const send = (event: string, payload: unknown = null) =>
    heard.get(event)?.({ event, id: 0, payload });
  return { reload, adopt, stop, send, heard };
}

describe('the Tick card', () => {
  it('reads the new tick over unsaved changes after a #profile load', async () => {
    // You changed the Warning text and did not save it yet.
    const { reload, adopt, send } = await card(() => true);
    // #profile load sends the loaded tick, then says it replaced the
    // whole profile.
    send('vosh://tick-config-changed', tick({ interval_secs: 35, auto_fire: 'score' }));
    expect(adopt).not.toHaveBeenCalled();
    send('vosh://ui-config-replaced');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('reads the new tick after a switch, a reset, or an import while clean', async () => {
    const { reload, send } = await card(() => false);
    send('vosh://ui-config-replaced');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('takes a change from elsewhere only while the draft is clean', async () => {
    let dirty = false;
    const { adopt, reload, send } = await card(() => dirty);
    const changed = tick({ warn_at_secs: 10 });
    send('vosh://tick-config-changed', changed);
    expect(adopt).toHaveBeenCalledWith(changed);
    dirty = true;
    send('vosh://tick-config-changed', tick({ warn_at_secs: 12 }));
    expect(adopt).toHaveBeenCalledTimes(1);
    expect(reload).not.toHaveBeenCalled();
  });

  it('stops listening when the card goes away', async () => {
    const { stop, heard } = await card(() => false);
    expect(heard.size).toBe(2);
    stop();
    expect(heard.size).toBe(0);
  });
});
