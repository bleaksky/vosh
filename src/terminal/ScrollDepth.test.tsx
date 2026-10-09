import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScrollDepth } from './ScrollDepth';

// The depth chip as markup. Effects do not run here, so the native
// scroll listener never starts and the native depth stays at 0.

const native = vi.hoisted(() => ({ on: false }));
vi.mock('./terminalRenderer', () => ({ nativeSurfaceEnabled: () => native.on }));

beforeEach(() => {
  native.on = false;
});

describe('ScrollDepth', () => {
  it('shows how far back the xterm history pane reads', () => {
    const html = renderToStaticMarkup(<ScrollDepth history={{ back: 54, max: 78 }} />);
    expect(html).toContain('class="ov-depth"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain('<span>54</span><span class="ov-depth-of">/ 78</span>');
  });

  it('hides at the tail and with no history', () => {
    expect(renderToStaticMarkup(<ScrollDepth history={{ back: 0, max: 78 }} />)).toBe('');
    expect(renderToStaticMarkup(<ScrollDepth history={{ back: 3, max: 0 }} />)).toBe('');
    expect(renderToStaticMarkup(<ScrollDepth history={null} />)).toBe('');
  });

  it('drops below the find bar while find is open', () => {
    const html = renderToStaticMarkup(<ScrollDepth findOpen history={{ back: 54, max: 78 }} />);
    expect(html).toContain('class="ov-depth is-below-find"');
  });

  it('reads the native depth, not the history pane, when the native surface draws', () => {
    native.on = true;
    expect(renderToStaticMarkup(<ScrollDepth history={{ back: 54, max: 78 }} />)).toBe('');
  });
});
