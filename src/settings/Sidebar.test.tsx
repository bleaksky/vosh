import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { Sidebar } from './Sidebar';

// The nav as the boards draw it. Effects do not run in a markup render,
// so the Find listener never reaches the app.

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

function draw(group: Parameters<typeof Sidebar>[0]['group']): string {
  return renderToStaticMarkup(
    <Sidebar group={group} onNavigate={() => undefined} pathB={false} mac />,
  );
}

/** Each nav row, its link, whether it starts a cluster, whether it is
 *  the page shown, its icon path and its label. */
function rows(html: string) {
  return [
    ...html.matchAll(
      /<a href="#(\w+)" class="st-nav-item"( data-cluster="")?( aria-current="page")?><svg[^>]*>(.*?)<\/svg><span class="st-nav-label">([^<]*)<\/span><\/a>/g,
    ),
  ].map(([, id, cluster, current, icon, label]) => ({
    id,
    cluster: !!cluster,
    current: !!current,
    icon,
    label,
  }));
}

describe('the Settings sidebar', () => {
  it('draws the eleven groups in four clusters, with no headings', () => {
    const drawn = rows(draw('general'));
    expect(drawn.map((r) => r.label)).toEqual([
      'General',
      'Appearance',
      'Accessibility',
      'Layout',
      'Vitals',
      'Prompt',
      'Input',
      'Automation',
      'Scripts',
      'Logs',
      'Characters',
    ]);
    // A gap starts Layout, Automation and Logs.
    expect(drawn.filter((r) => r.cluster).map((r) => r.label)).toEqual([
      'Layout',
      'Automation',
      'Logs',
    ]);
  });

  it('names the search shortcut apart, with its keycaps out of the name', () => {
    const html = draw('general');
    expect(html).toMatch(/role="combobox"[^>]*aria-keyshortcuts="Meta\+F"/);
    expect(html).toContain('<span class="st-search-keys" aria-hidden="true">');
  });

  it('gives each group its own glyph, and Prompt the terminal glyph Help draws for Play', () => {
    const drawn = rows(draw('general'));
    expect(new Set(drawn.map((r) => r.icon)).size).toBe(drawn.length);
    expect(drawn.find((r) => r.id === 'prompt')?.icon).toBe(
      '<rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2"></rect><path d="M4.75 6.25L6.75 8l-2 1.75M8.75 10h2.5"></path>',
    );
  });

  it('marks Scripts with the code glyph and as the page shown', () => {
    const scripts = rows(draw('scripts')).find((r) => r.id === 'scripts');
    expect(scripts?.current).toBe(true);
    expect(scripts?.icon).toBe(
      '<path d="M5.25 4.75L2 8l3.25 3.25M10.75 4.75L14 8l-3.25 3.25M9.25 3l-2.5 10"></path>',
    );
    expect(rows(draw('scripts')).filter((r) => r.current)).toHaveLength(1);
  });
});
