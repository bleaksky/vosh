import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { galleryThemes } from '../../../../lib/themeThumb';
import { BUILTIN_THEMES, customToAppTheme } from '../../../../lib/themes';
import { ThemeGallery } from './ThemeGallery';

const custom = (id: string, label: string) =>
  customToAppTheme({ id, label, description: '', xterm: {}, chrome: {} });

/** Each radio's value and the caption its label reads. The thumbnail
 *  is hidden from assistive tech, so the caption is the radio's name. */
function radios(html: string): { value: string; name: string }[] {
  return [
    ...html.matchAll(/value="([^"]*)"[^>]*\/>.*?<span class="st-theme-name">([^<]*)<\/span>/g),
  ].map((m) => ({ value: m[1], name: m[2] }));
}

describe('ThemeGallery', () => {
  it('names every radio, a custom theme with a blank name included', () => {
    const themes = galleryThemes(BUILTIN_THEMES, [custom('nord-copy', ''), custom('dusk', '  ')]);
    const html = renderToStaticMarkup(
      <ThemeGallery themes={themes} selected="nord" onPick={() => {}} />,
    );
    const list = radios(html);
    expect(list).toHaveLength(themes.length);
    for (const radio of list) expect(radio.name.trim()).not.toBe('');
    expect(list.slice(-2)).toEqual([
      { value: 'nord-copy', name: 'nord-copy' },
      { value: 'dusk', name: 'dusk' },
    ]);
  });
});
