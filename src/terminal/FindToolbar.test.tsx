import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { FindToolbar } from './FindToolbar';

describe('FindToolbar', () => {
  it('names the key of each button that has one', () => {
    const html = renderToStaticMarkup(
      <FindToolbar onFindNext={() => false} onFindPrevious={() => false} onClose={() => {}} />,
    );
    const keyed = [...html.matchAll(/aria-label="([^"]*)"[^>]*aria-keyshortcuts="([^"]*)"/g)].map(
      (m) => `${m[1]} ${m[2]}`,
    );
    expect(keyed).toEqual(['Previous match Shift+Enter', 'Next match Enter', 'Close find Escape']);
  });
});
