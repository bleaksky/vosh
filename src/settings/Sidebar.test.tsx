import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { Sidebar } from './Sidebar';

// The nav as it draws. Effects do not run in a markup render,
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

/** Each nav row, its link, whether it is the page shown, its icon path
 *  and its label. */
function rows(html: string) {
  return [
    ...html.matchAll(
      /<a href="#(\w+)" class="st-nav-item"( aria-current="page")?><svg[^>]*>(.*?)<\/svg><span class="st-nav-label">([^<]*)<\/span><\/a>/g,
    ),
  ].map(([, id, current, icon, label]) => ({ id, current: !!current, icon, label }));
}

describe('the Settings sidebar', () => {
  it('draws the seven groups in board order, Scripts between Automation and Characters', () => {
    expect(rows(draw('general')).map((r) => r.label)).toEqual([
      'General',
      'Appearance',
      'Layout',
      'Input',
      'Automation',
      'Scripts',
      'Characters',
    ]);
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
