import { describe, expect, it } from 'vitest';
import { crashNotice, errorText } from './crashNotice';

// The crash notice an uncaught error paints. At boot it takes the top of
// the window. Once the app has mounted it keeps to the corner below the
// title band, clear of the command line at the bottom. Errors that follow
// count into it, and Close or Escape takes it away. A small stand in DOM
// holds just what the notice touches.

class El {
  readonly tag: string;
  readonly style = { cssText: '' };
  readonly attrs = new Map<string, string>();
  readonly children: El[] = [];
  readonly clicks: (() => void)[] = [];
  parent: El | null = null;
  private text = '';
  constructor(tag: string) {
    this.tag = tag;
  }
  setAttribute(name: string, value: string) {
    this.attrs.set(name, value);
  }
  get textContent(): string {
    return this.text + this.children.map((c) => c.textContent).join('');
  }
  set textContent(text: string) {
    this.text = text;
  }
  append(...els: El[]) {
    for (const el of els) this.appendChild(el);
  }
  appendChild(el: El) {
    el.parent = this;
    this.children.push(el);
    return el;
  }
  remove() {
    if (!this.parent) return;
    this.parent.children.splice(this.parent.children.indexOf(this), 1);
    this.parent = null;
  }
  get isConnected(): boolean {
    return this.parent !== null;
  }
  addEventListener(_type: string, cb: () => void) {
    this.clicks.push(cb);
  }
  /** Every element under this one with tag `tag`. */
  all(tag: string): El[] {
    return this.children.flatMap((c) => [...(c.tag === tag ? [c] : []), ...c.all(tag)]);
  }
}

function page(mountedChildren = 0) {
  const body = new El('body');
  // Body is the root of the stand in, so it counts as connected.
  body.parent = new El('html');
  const keys = new Set<(event: KeyboardEvent) => void>();
  const root = { childElementCount: mountedChildren };
  const doc = {
    body,
    createElement: (tag: string) => new El(tag),
    getElementById: (id: string) => (id === 'root' ? root : null),
    addEventListener: (_: string, cb: (event: KeyboardEvent) => void) => keys.add(cb),
    removeEventListener: (_: string, cb: (event: KeyboardEvent) => void) => keys.delete(cb),
  };
  const copied: string[] = [];
  const notice = crashNotice(doc as unknown as Document, async (text) => {
    copied.push(text);
  });
  const key = (k: string) => {
    for (const cb of [...keys]) cb({ key: k } as KeyboardEvent);
  };
  const shown = () => body.children;
  const press = (label: string) =>
    shown()[0]
      ?.all('button')
      .find((b) => b.textContent === label)
      ?.clicks.forEach((cb) => cb());
  return { notice, root, body, keys, key, shown, press, copied };
}

const ERR = new TypeError(
  "undefined is not an object (evaluating 'this._renderer.value.dimensions')",
);

describe('the crash notice', () => {
  it('takes the top of the window when the app never mounted', () => {
    const { notice, shown } = page(0);
    notice.show('render failed', ERR);
    expect(shown()).toHaveLength(1);
    const box = shown()[0];
    expect(box?.attrs.get('role')).toBe('alert');
    expect(box?.style.cssText).toContain('top:12px;left:12px;right:12px');
    expect(box?.style.cssText).not.toContain('bottom');
    expect(box?.textContent).toContain('Something went wrong');
    expect(box?.textContent).toContain('Vosh may not have started.');
    expect(box?.textContent).toContain('render failed: undefined is not an object');
  });

  it('keeps to the corner clear of the command line once the app mounted', () => {
    const { notice, shown } = page(1);
    notice.show('uncaught error', ERR);
    const css = shown()[0]?.style.cssText ?? '';
    expect(css).toContain('top:44px;right:12px');
    expect(css).not.toContain('bottom');
    expect(css).not.toContain('left');
    expect(shown()[0]?.textContent).toContain('You can keep playing.');
  });

  it('counts the errors that follow into one notice', () => {
    const { notice, shown } = page(1);
    notice.show('uncaught error', ERR);
    notice.show('uncaught error', ERR);
    notice.show('unhandled rejection', 'lost the session');
    expect(shown()).toHaveLength(1);
    const text = shown()[0]?.textContent ?? '';
    expect(text).toContain('Something went wrong 3 times');
    // It shows the newest.
    expect(text).toContain('unhandled rejection: lost the session');
  });

  it('closes on Close and on Escape, and a later error shows afresh', () => {
    const { notice, shown, press, key, keys } = page(1);
    notice.show('uncaught error', ERR);
    press('Close');
    expect(shown()).toHaveLength(0);
    expect(keys.size).toBe(0);

    notice.show('uncaught error', ERR);
    notice.show('uncaught error', ERR);
    key('a');
    expect(shown()).toHaveLength(1);
    key('Escape');
    expect(shown()).toHaveLength(0);

    notice.show('uncaught error', 'once more');
    expect(shown()[0]?.textContent).toContain('Something went wrong');
    expect(shown()[0]?.textContent).not.toContain('times');
  });

  it('copies every error it holds', async () => {
    const { notice, press, copied, shown } = page(1);
    notice.show('uncaught error', 'first');
    notice.show('unhandled rejection', 'second');
    press('Copy details');
    await Promise.resolve();
    await Promise.resolve();
    expect(copied).toEqual(['uncaught error: first\n\nunhandled rejection: second']);
    expect(
      shown()[0]
        ?.all('button')
        .map((b) => b.textContent),
    ).toEqual(['Copied', 'Close']);
  });

  it('names an error by its message and stack', () => {
    const err = new Error('boom');
    err.stack = 'Error: boom\n    at _sync';
    expect(errorText('uncaught error', err)).toBe(
      'uncaught error: boom\nError: boom\n    at _sync',
    );
    expect(errorText('unhandled rejection', 7)).toBe('unhandled rejection: 7');
  });
});
