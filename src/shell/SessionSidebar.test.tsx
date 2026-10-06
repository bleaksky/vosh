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

/** Whether you hold ⌘, faked. */
const mod = vi.hoisted(() => ({ held: false }));

vi.mock('./useModHeld', () => ({ useModHeld: () => mod.held }));

vi.mock('../stores/session/sessionRowStore', async (actual) => {
  const store = await actual<typeof import('../stores/session/sessionRowStore')>();
  return {
    ...store,
    useSessionRow: (id: number) => ({ ...store.getSessionRow(id), ...states.get(id) }),
  };
});

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
      onClose={() => undefined}
      onHide={() => undefined}
      onCaret={() => undefined}
    />,
  );
}

/** Each row button, in order. */
const buttons = (html: string) =>
  html.match(/<button[^>]*class="shell-sessions-row[^"]*"[^]*?<\/button>/g) ?? [];

afterEach(() => {
  states.clear();
  mod.held = false;
  vi.unstubAllGlobals();
});

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

  it('gives each row a close button beside it, out of the Tab order', () => {
    const html = draw(rows, 1);
    const slots = html.match(/<li class="shell-sessions-slot">[^]*?<\/li>/g) ?? [];
    expect(slots).toHaveLength(3);
    for (const slot of slots) {
      expect(slot).toMatch(
        /<\/button><button type="button" class="shell-sessions-close" aria-label="Close session" tabindex="-1"><svg[^]*<\/button><\/li>$/,
      );
    }
  });

  it('marks the session the selection moves to', () => {
    const [tolliver, orla] = buttons(draw(rows, 2));
    expect(tolliver).not.toContain('aria-current');
    expect(orla).toContain('aria-current="true"');
  });

  it('marks a row behind for new lines and alerts, with the dot in the meta place', () => {
    states.set(1, { lines: true, alert: true });
    states.set(2, { lines: true, alert: true });
    states.set(3, { lines: true, playing: true });
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

  it('shows the link of each session as a glyph, and dims one not connected', () => {
    const glyphs = [
      row(1, { character: 'Tolliver' }),
      row(2, { port: 1825 }),
      row(3, {}),
      row(4, { character: 'Tolliver' }),
      row(5, { character: 'Orla', port: 1825, connected: false }),
    ];
    states.set(3, { link: 'dialing' });
    states.set(4, { link: 'failed' });
    const [, login, dialing, failed, off] = buttons(draw(glyphs, 1));
    const glyph = (kind: string, words: string) =>
      `<span class="shell-sessions-glyph is-${kind}" role="img" aria-label="${words}"><svg`;
    expect(login).toContain('1825</span>' + glyph('hand', 'Logging in'));
    expect(dialing).toContain(glyph('spinner', 'Connecting'));
    expect(failed).toContain(glyph('triangle', 'Connect again'));
    expect(off).toContain('class="shell-sessions-row is-off"');
    expect(off).toContain('<span class="shell-sessions-meta">1825</span>');
    expect(off).not.toContain('shell-sessions-glyph');
  });

  it('numbers the first nine rows while you hold ⌘, in place of the meta and the glyph', () => {
    vi.stubGlobal('navigator', { userAgent: 'Macintosh' });
    const many = [
      row(1, { character: 'Tolliver' }),
      row(2, { character: 'Orla', port: 1825 }),
      ...Array.from({ length: 8 }, (_, i) => row(i + 3, { port: i % 2 ? 1825 : 1848 })),
    ];
    states.set(4, { link: 'failed' });
    const quiet = buttons(draw(many, 1));
    expect(quiet.join('')).not.toContain('is-key');
    mod.held = true;
    const numbered = buttons(draw(many, 1));
    numbered.slice(0, 9).forEach((button, i) => {
      expect(button).toMatch(
        new RegExp(`</span><span class="shell-sessions-meta is-key">⌘${i + 1}</span></button>$`),
      );
      expect(button).not.toContain('shell-sessions-glyph');
    });
    // Orla's port and the failed row's triangle give way to the number.
    expect(quiet[1]).toContain('<span class="shell-sessions-meta">1825</span>');
    expect(numbered[1]).not.toContain('<span class="shell-sessions-meta">1825</span>');
    expect(quiet[3]).toContain('shell-sessions-glyph is-triangle');
    // The tenth row has no key and keeps its port.
    expect(numbered[9]).not.toContain('is-key');
    expect(numbered[9]).toContain(
      '<span class="shell-sessions-world">The Forsaken Lands</span>1825',
    );
  });

  it('names the key with Ctrl on Windows and Linux', () => {
    vi.stubGlobal('navigator', { userAgent: 'Windows NT 10.0' });
    mod.held = true;
    const [tolliver] = buttons(draw(rows, 1));
    expect(tolliver).toContain('<span class="shell-sessions-meta is-key">Ctrl+1</span>');
  });
});
