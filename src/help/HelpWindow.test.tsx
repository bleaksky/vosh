import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { HelpApp } from './HelpWindow';

// The Help window reads the config and follows the theme in effects, which
// a static render never runs. The mocks only keep the imports quiet.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(undefined)) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    show: () => Promise.resolve(),
    setFocus: () => Promise.resolve(),
  }),
}));

describe('the help window', () => {
  it('lets Tab reach the article, named for the topic it shows', () => {
    const html = renderToStaticMarkup(<HelpApp />);
    const scroller = /<div[^>]*class="hp-scroll"[^>]*>/.exec(html)?.[0] ?? '';
    expect(scroller).toContain('tabindex="0"');
    expect(scroller).toContain('role="region"');
    expect(scroller).toMatch(/aria-label="[^"]+"/);
  });
});
