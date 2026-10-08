import { Children, isValidElement, type ReactElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// Board 5 of the Sessions review (Q10): Orla's session (1) is selected,
// a tell reaches Tolliver's (2) behind it, and the corner shows the
// alert on the update notice recipe with the accent dot and Show.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const calls = vi.hoisted(() => [] as unknown[]);

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args: unknown) => {
    calls.push([cmd, args]);
    if (cmd === 'sessions_list') return [];
    return null;
  },
}));

const fire = (event: string, payload: unknown) => {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
};

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const row = (id: number, character: string) => ({
  id,
  name: null,
  character,
  host: 'play.theforsakenlands.com',
  port: 1825,
  tls: false,
  profile: 'Default',
  connected: true,
  since: null,
  selected: id === 1,
});

/** Start the stores with Orla in front, then ring a tell in Tolliver's
 *  session, as alert.rs sends it while Vosh is in front. */
async function seed(label: string | null = 'Tolliver') {
  const { startSessionsStore } = await import('../../stores/session/sessionsStore');
  const { startAlertNoticeStore } = await import('../../stores/session/alertNoticeStore');
  startSessionsStore();
  startAlertNoticeStore();
  await settle();
  fire('vosh://sessions-changed', [row(1, 'Orla'), row(2, 'Tolliver')]);
  fire('session://alert', {
    session: 2,
    title: 'Tell from Maren',
    label,
    words: null,
    sound: null,
    banner: false,
    notice: true,
    source: 'preset:alert_tells',
    owner: null,
  });
}

async function corner() {
  const { CornerNotices } = await import('./CornerNotices');
  return renderToStaticMarkup(<CornerNotices />);
}

/** The buttons of the card as AlertNotice draws it now. */
async function buttons(): Promise<ReactElement<{ onClick: () => void }>[]> {
  const { AlertNotice } = await import('./AlertNotice');
  let card: ReactNode = null;
  const Capture = () => (card = AlertNotice());
  renderToStaticMarkup(<Capture />);
  const found: ReactElement<{ onClick: () => void }>[] = [];
  const walk = (node: ReactNode) =>
    Children.forEach(node, (child) => {
      if (!isValidElement<{ children?: ReactNode; onClick: () => void }>(child)) return;
      if (child.type === 'button') found.push(child);
      walk(child.props.children);
    });
  walk(card);
  return found;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  calls.length = 0;
});

describe('the alert notice', () => {
  it('names the alert and its session with Show alone, in the accent', async () => {
    await seed();
    const html = await corner();
    expect(html).toContain('<div class="ov-update" role="status" aria-live="polite">');
    expect(html).toContain('<span class="ov-update-msg">Tell from Maren</span>');
    expect(html).toContain('<span class="ov-update-meta">to Tolliver</span>');
    expect(html.match(/<button/g)).toHaveLength(1);
    expect(html).toMatch(/<button type="button" class="ov-button">Show<\/button>/);
    expect(html).not.toContain('is-primary');
  });

  it('leaves the meta out when the session has no label', async () => {
    await seed(null);
    const html = await corner();
    expect(html).toContain('Tell from Maren');
    expect(html).not.toContain('ov-update-meta');
  });

  it('sits under the reconnect notice and over a preset fix', async () => {
    await seed();
    const { showPresetFix } = await import('../../stores/presetFixStore');
    showPresetFix({
      told: [{ preset: 'disarm_buff_fade', trigger: 'disarm.secondary', row: 'send' }],
      removed: [],
    });
    const { CornerNotices } = await import('./CornerNotices');
    const html = renderToStaticMarkup(<CornerNotices reconnect={<p>reconnect</p>} />);
    const order = ['<p>reconnect</p>', 'Tell from Maren', 'ov-update is-warn'].map((s) =>
      html.indexOf(s),
    );
    expect(order[0]).toBeGreaterThan(0);
    expect(order).toEqual([...order].sort((a, b) => a - b));
  });

  it('draws nothing while no alert waits', async () => {
    expect(await corner()).toBe('<div class="ov-corner"></div>');
  });

  it('selects the session behind with Show', async () => {
    await seed();
    const [show] = await buttons();
    show.props.onClick();
    await settle();
    expect(calls).toContainEqual(['session_select', { session: 2 }]);
    expect(await corner()).toBe('<div class="ov-corner"></div>');
  });
});
