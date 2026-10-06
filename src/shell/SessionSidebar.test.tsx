import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import type { SessionRowState } from '../stores/session/sessionRowStore';
import { SessionSidebar } from './SessionSidebar';

// The sessions sidebar of boards 2 and 3. Each row reads its session as
// sessionLabel names it, the selected one marked current, a port that is
// not the world port in quiet meta, and a row with neither a name nor a
// character named by its world with the port kept apart. A row takes the
// look the row store gives it, faked here for each session.

/** What the row store says of each session, by id. A session it names
 *  nothing for reads quiet. */
const states = vi.hoisted(() => new Map<number, Partial<SessionRowState>>());

vi.mock('../stores/session/sessionRowStore', async (actual) => ({
  ...(await actual<typeof import('../stores/session/sessionRowStore')>()),
  useSessionRow: (id: number) => ({ lines: false, alert: false, ...states.get(id) }),
}));

const PLAY = 'play.theforsakenlands.com';

function row(id: number, fields: Partial<SessionRow>): SessionRow {
  return {
    id,
    name: null,
    character: null,
    host: PLAY,
    port: 1848,
    tls: false,
    profile: 'Default',
    connected: true,
    selected: false,
    ...fields,
  };
}

function draw(rows: SessionRow[], selected: number): string {
  return renderToStaticMarkup(
    <SessionSidebar
      rows={rows}
      selected={selected}
      onSelect={() => undefined}
      onNewSession={() => undefined}
      onHide={() => undefined}
      onCaret={() => undefined}
    />,
  );
}

/** Each row button, in order. */
const buttons = (html: string) =>
  html.match(/<button[^>]*class="shell-sessions-row[^"]*"[^]*?<\/button>/g) ?? [];

afterEach(() => states.clear());

describe('the sessions sidebar', () => {
  const rows = [
    row(1, { character: 'Tolliver' }),
    row(2, { character: 'Orla', port: 1825 }),
    row(3, { port: 1825 }),
  ];

  it('holds New session and Hide sessions over the SESSIONS header', () => {
    const html = draw(rows, 1);
    expect(html).toContain('<aside class="shell-sessions st-controls" aria-label="Sessions">');
    expect(html).toContain('<div class="shell-sessions-top" data-tauri-drag-region="true">');
    expect(html).toContain('aria-label="New session"');
    expect(html).toContain('aria-label="Hide sessions"');
    expect(html).toContain('<h2 class="shell-sessions-head">Sessions</h2>');
  });

  it('lists every session in order, the selected one current', () => {
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    expect(tolliver).toContain('aria-current="true"');
    expect(tolliver).toContain('title="Tolliver on The Forsaken Lands"');
    expect(tolliver).toContain('<span class="shell-sessions-name">Tolliver</span>');
    expect(tolliver).not.toContain('shell-sessions-meta');
    expect(orla).not.toContain('aria-current');
    expect(orla).toContain(
      '<span class="shell-sessions-name">Orla</span><span class="shell-sessions-meta">1825</span>',
    );
    expect(build).toContain(
      '<span class="shell-sessions-name is-world"><span class="shell-sessions-world">The Forsaken Lands</span>1825</span>',
    );
  });

  it('marks the session the selection moves to', () => {
    const [tolliver, orla] = buttons(draw(rows, 2));
    expect(tolliver).not.toContain('aria-current');
    expect(orla).toContain('aria-current="true"');
  });

  it('marks a row behind for new lines and alerts, with the dot in the meta place', () => {
    states.set(1, { lines: true, alert: true });
    states.set(2, { lines: true, alert: true });
    states.set(3, { lines: true });
    const [tolliver, orla, build] = buttons(draw(rows, 1));
    // The selected row shows neither.
    expect(tolliver).toContain('class="shell-sessions-row"');
    expect(tolliver).not.toContain('shell-sessions-glyph');
    expect(orla).toContain('class="shell-sessions-row is-new"');
    expect(orla).toContain(
      '<span class="shell-sessions-name">Orla</span><span class="shell-sessions-glyph is-dot" role="img" aria-label="Something for you"><svg',
    );
    expect(orla).not.toContain('shell-sessions-meta');
    expect(build).toContain('class="shell-sessions-row is-new"');
    expect(build).not.toContain('shell-sessions-glyph');
  });
});
