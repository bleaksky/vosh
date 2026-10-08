import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type UiConfig } from '../../ipc/uiConfig';
import { InputPage } from './InputPage';

// The page saves through the Tauri bridge. These tests only draw it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

function draw(fields: Partial<UiConfig> = {}): string {
  const config = {
    ...normalizeUiConfig({
      theme: 'obsidian-ember',
      auto_update: false,
      font_family: 'Menlo',
      font_size: 14,
      tracked_affects: [],
      enabled_presets: [],
    }),
    ...fields,
  };
  return renderToStaticMarkup(
    <InputPage
      target={{ group: 'input' }}
      navSeq={0}
      config={config}
      setConfig={() => undefined}
      onError={() => undefined}
      pathB={false}
      navigate={() => undefined}
      setLeaveGuard={() => undefined}
    />,
  );
}

const between = (html: string, from: string, to: string) =>
  html.slice(html.indexOf(from), html.indexOf(to));
const labels = (html: string) =>
  [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
const segments = (html: string) =>
  [...html.matchAll(/class="st-seg-item"[^>]*>([^<]*)</g)].map((m) => m[1]);
const pressed = (html: string) =>
  [...html.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);

describe('InputPage', () => {
  it('splits Sent commands from Command line, in the order the board draws', () => {
    const html = draw();
    const sent = between(html, 'data-st-anchor="sent"', 'data-st-anchor="command-line"');
    expect(labels(sent)).toEqual([
      'Mark before your commands',
      'Mark color',
      'Command color',
      'Dim sent commands',
      'Use the same mark in the command line',
      'Show the commands your macros send',
    ]);
    const line = between(html, 'data-st-anchor="command-line"', 'data-st-anchor="writing"');
    expect(labels(line)).toEqual([
      'Caret shape',
      'Keep last command',
      'Check spelling when you chat',
    ]);
    expect(sent).toContain('Vosh leaves it out after a prompt that already ends in &gt;.');
    expect(sent).toContain('Your commands draw faint, so the game’s lines stand out.');
    expect(sent).toContain('The line you type in starts with your mark.');
  });

  it('offers Off, ›, > and Your own, with the field only for your own', () => {
    const chevron = draw();
    const mark = between(chevron, 'data-st-anchor="mark-commands"', 'data-st-anchor="mark-color"');
    expect(segments(mark)).toEqual(['Off', '›', '&gt;', 'Your own']);
    expect(pressed(mark)).toEqual(['›']);
    expect(mark).not.toContain('aria-label="Your own mark"');

    const own = draw({ input_echo_mark: 'own', input_echo_mark_text: 'you:' });
    const ownMark = between(own, 'data-st-anchor="mark-commands"', 'data-st-anchor="mark-color"');
    expect(pressed(ownMark)).toEqual(['Your own']);
    expect(ownMark).toMatch(
      /aria-label="Your own mark"[^>]*value="you:"|value="you:"[^>]*aria-label="Your own mark"/,
    );
  });

  it('shows the theme’s bright black as the mark’s Theme default', () => {
    const html = draw();
    const color = between(html, 'data-st-anchor="mark-color"', 'data-st-anchor="sent-color"');
    expect(color).toContain('placeholder="Theme default"');
    expect(color).toContain('#5f5a55');
  });

  it('draws dim off and the line mark on by default', () => {
    const html = draw();
    const checked = (anchor: string, next: string) =>
      between(html, `data-st-anchor="${anchor}"`, `data-st-anchor="${next}"`).includes(
        'checked=""',
      );
    expect(checked('sent-dim', 'mark-line')).toBe(false);
    expect(checked('mark-line', 'echo-macros')).toBe(true);
  });
});
