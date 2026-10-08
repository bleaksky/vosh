import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { HELP_TOPICS } from '../help/helpContent';
import general from '../settings/general/GeneralPage.tsx?raw';
import inputPrompt from '../settings/input/InputPrompt.tsx?raw';
import layout from '../settings/layout/LayoutPage.tsx?raw';
import logs from '../settings/logs/LogsPage.tsx?raw';
import appearance from '../settings/appearance/AppearancePage.tsx?raw';
import characters from '../settings/characters/CharactersPage.tsx?raw';
import { Section } from './Section';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn(() => Promise.resolve()) }));

// A Settings section that has a help topic carries a book button at the
// end of its heading, after its meta (the HelpLinks board).

describe('a Settings section with help', () => {
  it('ends its heading with a book button named for what it opens', () => {
    const html = renderToStaticMarkup(
      <Section
        title="Prompt"
        actions={<span className="st-meta">Saved for Ilsabet</span>}
        help={{ topic: 'shape.prompt-show', subject: 'your prompt' }}
      />,
    );
    expect(html).toMatch(
      /<div class="st-section-actions"><span class="st-meta">Saved for Ilsabet<\/span><button type="button" aria-label="Help on your prompt" class="st-icon-button"><svg/,
    );
  });

  it('draws no button without a topic', () => {
    const html = renderToStaticMarkup(<Section title="Prompt" />);
    expect(html).not.toContain('st-section-actions');
    expect(html).not.toContain('Help on');
  });
});

describe('the Settings sections that link to help', () => {
  // Each `help={{ topic: ... }}` in Settings names a topic that exists,
  // so a book button never opens Help on nothing.
  const files: Record<string, string> = {
    general,
    inputPrompt,
    layout,
    logs,
    appearance,
    characters,
  };
  const links = Object.entries(files).flatMap(([file, source]) =>
    [...source.matchAll(/help=\{\{ topic: '([^']+)', subject: '([^']+)' \}\}/g)].map((m) => ({
      file,
      topic: m[1],
      subject: m[2],
    })),
  );

  it('name topics that exist', () => {
    expect(links.map((l) => l.topic).sort()).toEqual([
      'characters-and-data.profiles',
      'characters-and-data.search-logs',
      'make-it-yours.switch-themes',
      'play.scroll-back',
      'shape.group-affects',
      'shape.prompt-show',
      'tick.tick-timer',
    ]);
    for (const link of links) {
      expect(
        HELP_TOPICS.some((t) => t.id === link.topic),
        `${link.file} ${link.topic}`,
      ).toBe(true);
    }
  });
});
