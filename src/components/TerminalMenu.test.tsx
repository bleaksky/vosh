import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TerminalMenu } from './TerminalMenu';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
// The menu asks storage which renderer draws the terminal.
vi.stubGlobal('localStorage', {
  getItem: () => null,
  setItem: () => undefined,
  removeItem: () => undefined,
});

describe('the terminal menu', () => {
  it('offers Customize prompt… first, apart from the rest, on any row (P1)', () => {
    const html = renderToStaticMarkup(
      <TerminalMenu
        x={10}
        y={10}
        termRef={{ current: null }}
        inputRef={{ current: null }}
        onOpenFind={() => {}}
        onCustomizePrompt={() => {}}
        onClose={() => {}}
      />,
    );
    const labels = [...html.matchAll(/class="ov-menu-label">([^<]*)</g)].map((m) => m[1]);
    expect(labels.slice(0, 2)).toEqual(['Customize prompt…', 'Copy']);
    // A separator stands between it and Copy.
    const first = html.indexOf('Customize prompt…');
    const sep = html.indexOf('role="separator"');
    expect(sep).toBeGreaterThan(first);
    expect(sep).toBeLessThan(html.indexOf('>Copy<'));
  });
});
