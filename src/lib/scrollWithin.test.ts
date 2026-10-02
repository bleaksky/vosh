import { describe, expect, it } from 'vitest';
import { scrollingAncestor, scrollWithin } from './scrollWithin';

// Node has no DOM or layout. Plain objects stand in for the elements:
// each one has a parent, a computed style, a rect, and the scroll
// numbers, and records every scroll write so a test can say which box
// moved and which stayed.

const DEFAULTS: Record<string, string> = {
  overflowX: 'visible',
  overflowY: 'visible',
  scrollPaddingTop: 'auto',
  scrollPaddingBottom: 'auto',
  scrollPaddingLeft: 'auto',
  scrollPaddingRight: 'auto',
  scrollMarginTop: '0px',
  scrollMarginBottom: '0px',
  scrollMarginLeft: '0px',
  scrollMarginRight: '0px',
};

interface Spec {
  style?: Record<string, string>;
  top?: number;
  height?: number;
  left?: number;
  width?: number;
  clientHeight?: number;
  scrollHeight?: number;
  clientWidth?: number;
  scrollWidth?: number;
  clientTop?: number;
  clientLeft?: number;
  scrollTop?: number;
  scrollLeft?: number;
}

interface FakeDoc {
  body: FakeEl | null;
  documentElement: FakeEl | null;
  defaultView: { getComputedStyle: (el: FakeEl) => Record<string, string> };
}

class FakeEl {
  writes: [string, number][] = [];
  private top_: number;
  private left_: number;
  constructor(
    readonly ownerDocument: FakeDoc,
    readonly parentElement: FakeEl | null,
    private readonly spec: Spec,
  ) {
    this.top_ = spec.scrollTop ?? 0;
    this.left_ = spec.scrollLeft ?? 0;
  }
  get style() {
    return { ...DEFAULTS, ...this.spec.style };
  }
  get clientHeight() {
    return this.spec.clientHeight ?? this.spec.height ?? 0;
  }
  get scrollHeight() {
    return this.spec.scrollHeight ?? this.clientHeight;
  }
  get clientWidth() {
    return this.spec.clientWidth ?? this.spec.width ?? 0;
  }
  get scrollWidth() {
    return this.spec.scrollWidth ?? this.clientWidth;
  }
  get clientTop() {
    return this.spec.clientTop ?? 0;
  }
  get clientLeft() {
    return this.spec.clientLeft ?? 0;
  }
  get scrollTop() {
    return this.top_;
  }
  set scrollTop(value: number) {
    this.writes.push(['scrollTop', value]);
    this.top_ = value;
  }
  get scrollLeft() {
    return this.left_;
  }
  set scrollLeft(value: number) {
    this.writes.push(['scrollLeft', value]);
    this.left_ = value;
  }
  getBoundingClientRect() {
    const top = this.spec.top ?? 0;
    const left = this.spec.left ?? 0;
    const height = this.spec.height ?? 0;
    const width = this.spec.width ?? 0;
    return { top, left, height, width, bottom: top + height, right: left + width };
  }
}

/** A page: the root element and the body, under which `add` hangs
 *  elements by parent. */
function page() {
  const doc: FakeDoc = {
    body: null,
    documentElement: null,
    defaultView: { getComputedStyle: (el) => el.style },
  };
  const add = (parent: FakeEl | null, spec: Spec = {}) => new FakeEl(doc, parent, spec);
  const html = add(null, { style: { overflowY: 'auto' }, height: 600, scrollHeight: 900 });
  const body = add(html, { style: { overflowY: 'auto' }, height: 600, scrollHeight: 900 });
  doc.documentElement = html;
  doc.body = body;
  // The window frame clips with overflow hidden but holds more than it
  // shows, so the DOM call would have scrolled it.
  const frame = add(body, {
    style: { overflowX: 'hidden', overflowY: 'hidden' },
    height: 600,
    scrollHeight: 700,
  });
  return { html, body, frame, add };
}

/** A list 200 tall at y 100 holding 1000 of rows, in the frame. */
function list(scrollTop = 0, style: Record<string, string> = {}) {
  const p = page();
  const box = p.add(p.frame, {
    style: { overflowY: 'auto', ...style },
    top: 100,
    height: 200,
    scrollHeight: 1000,
    scrollTop,
  });
  const row = (top: number, height = 20, spec: Spec = {}) => p.add(box, { top, height, ...spec });
  return { ...p, box, row };
}

const run = (el: FakeEl | null, block: 'start' | 'center' | 'nearest') =>
  scrollWithin(el as unknown as Element, { block });

const untouched = (...els: FakeEl[]) => els.every((el) => el.writes.length === 0);

describe('scrollWithin', () => {
  it('puts the element at the top of its box for start', () => {
    const t = list();
    run(t.row(600), 'start');
    expect(t.box.scrollTop).toBe(500);
    expect(untouched(t.frame, t.body, t.html)).toBe(true);
  });

  it('puts the element in the middle of its box for center', () => {
    const t = list();
    run(t.row(600), 'center');
    // The row's middle at 610 lands on the box's middle at 200.
    expect(t.box.scrollTop).toBe(410);
    expect(untouched(t.frame, t.body, t.html)).toBe(true);
  });

  it('lines a row below the box up on the bottom edge for nearest', () => {
    const t = list();
    run(t.row(600), 'nearest');
    // The row's bottom at 620 lands on the box's bottom at 300.
    expect(t.box.scrollTop).toBe(320);
  });

  it('lines a row above the box up on the top edge for nearest', () => {
    const t = list(400);
    run(t.row(50), 'nearest');
    expect(t.box.scrollTop).toBe(350);
  });

  it('leaves a row that already shows where it is for nearest', () => {
    const t = list(300);
    run(t.row(150), 'nearest');
    run(t.row(100), 'nearest');
    run(t.row(280), 'nearest');
    expect(t.box.scrollTop).toBe(300);
    expect(untouched(t.box, t.frame, t.body, t.html)).toBe(true);
  });

  it('leaves an element taller than the box that covers it for nearest', () => {
    const t = list(300);
    run(t.row(50, 400), 'nearest');
    expect(untouched(t.box)).toBe(true);
  });

  it('writes nothing when start finds the element already at the top', () => {
    const t = list(300);
    run(t.row(100), 'start');
    expect(untouched(t.box)).toBe(true);
  });

  it('centers a match at the end as far as the box goes and leaves the frame alone', () => {
    // The report. A search hit near the end of a long article asks to
    // center, the article is already at its end, and the frame slid.
    const t = list(800);
    run(t.row(280), 'center');
    expect(t.box.scrollTop).toBe(800);
    expect(untouched(t.box, t.frame, t.body, t.html)).toBe(true);
  });

  it('holds the box between its top and its end', () => {
    const top = list(100);
    run(top.row(20), 'center');
    expect(top.box.scrollTop).toBe(0);
    const end = list(700);
    run(end.row(290), 'start');
    expect(end.box.scrollTop).toBe(800);
  });

  it('counts the border at the top of the box', () => {
    const p = page();
    const box = p.add(p.frame, {
      style: { overflowY: 'auto' },
      top: 100,
      height: 202,
      clientHeight: 200,
      clientTop: 1,
      scrollHeight: 1000,
    });
    run(p.add(box, { top: 601, height: 20 }), 'start');
    expect(box.scrollTop).toBe(500);
  });

  it('honors the scroll margin on the element', () => {
    // A Help outline item lands 48 under the top of the article.
    const t = list();
    run(t.row(600, 20, { style: { scrollMarginTop: '48px' } }), 'start');
    expect(t.box.scrollTop).toBe(452);
  });

  it('honors the scroll padding on the box', () => {
    const t = list(0, { scrollPaddingTop: '8px', scrollPaddingBottom: '24px' });
    run(t.row(600), 'nearest');
    // The port ends 24 above the box's bottom, at 276.
    expect(t.box.scrollTop).toBe(344);
    run(t.row(100), 'start');
    expect(t.box.scrollTop).toBe(336);
  });

  it('reads a percent of scroll padding against the box', () => {
    const t = list(0, { scrollPaddingTop: '10%' });
    run(t.row(600), 'start');
    expect(t.box.scrollTop).toBe(480);
  });

  it('moves across the least that shows the element when the box scrolls across', () => {
    const p = page();
    const box = p.add(p.frame, {
      style: { overflowX: 'auto', overflowY: 'auto' },
      top: 0,
      height: 200,
      scrollHeight: 400,
      left: 0,
      width: 300,
      scrollWidth: 600,
    });
    run(p.add(box, { top: 50, height: 20, left: 350, width: 50 }), 'nearest');
    expect(box.scrollLeft).toBe(100);
    expect(box.scrollTop).toBe(0);
  });

  it('leaves scrollLeft alone when the box does not scroll across', () => {
    const p = page();
    const box = p.add(p.frame, {
      style: { overflowX: 'hidden', overflowY: 'auto' },
      top: 0,
      height: 200,
      scrollHeight: 400,
      width: 300,
      scrollWidth: 600,
    });
    run(p.add(box, { top: 300, height: 20, left: 350, width: 50 }), 'nearest');
    expect(box.scrollTop).toBe(120);
    expect(box.writes.filter(([axis]) => axis === 'scrollLeft')).toEqual([]);
  });

  it('does nothing for a missing element', () => {
    expect(() => scrollWithin(null, { block: 'nearest' })).not.toThrow();
    expect(() => scrollWithin(undefined, { block: 'center' })).not.toThrow();
  });
});

describe('scrollingAncestor', () => {
  it('picks the nearest box that scrolls and holds more than it shows', () => {
    const p = page();
    const outer = p.add(p.frame, {
      style: { overflowY: 'auto' },
      height: 400,
      scrollHeight: 900,
    });
    // Scrolls, but everything fits.
    const roomy = p.add(outer, { style: { overflowY: 'auto' }, height: 300, scrollHeight: 300 });
    // Holds more than it shows, but clips.
    const clipped = p.add(roomy, {
      style: { overflowY: 'hidden' },
      height: 100,
      scrollHeight: 250,
    });
    const row = p.add(clipped, { top: 500, height: 20 });
    expect(scrollingAncestor(row as unknown as Element)).toBe(outer);
    run(row, 'nearest');
    expect(outer.writes.length).toBe(1);
    expect(untouched(roomy, clipped, p.frame, p.body, p.html)).toBe(true);
  });

  it('takes overflow scroll as well as auto', () => {
    const p = page();
    const box = p.add(p.frame, { style: { overflowY: 'scroll' }, height: 100, scrollHeight: 200 });
    const row = p.add(box);
    expect(scrollingAncestor(row as unknown as Element)).toBe(box);
  });

  it('takes the inner box when two nested boxes both scroll', () => {
    const p = page();
    const outer = p.add(p.frame, { style: { overflowY: 'auto' }, height: 400, scrollHeight: 900 });
    const inner = p.add(outer, { style: { overflowY: 'auto' }, height: 100, scrollHeight: 300 });
    const row = p.add(inner);
    expect(scrollingAncestor(row as unknown as Element)).toBe(inner);
  });

  it('stops at the body, so the document never moves', () => {
    const p = page();
    // Only the body and the root element scroll above this row.
    const row = p.add(p.body, { top: 800, height: 20 });
    expect(scrollingAncestor(row as unknown as Element)).toBeNull();
    run(row, 'center');
    expect(untouched(p.body, p.html)).toBe(true);
  });
});

describe('scroll into view', () => {
  // The DOM call moves every scrolling ancestor, the window frame
  // included. Everything in src goes through scrollWithin instead.
  const sources = import.meta.glob<string>('../**/*.{ts,tsx}', {
    query: '?raw',
    import: 'default',
    eager: true,
  });
  const app = Object.keys(sources).filter(
    (path) => !/\.test\.tsx?$/.test(path) && !path.startsWith('../test/'),
  );

  it('reads the app sources', () => {
    // Vite names the files beside this one from here.
    expect(app).toContain('../HelpApp.tsx');
    expect(app).toContain('../components/help/HelpSidebar.tsx');
    expect(app).toContain('./scrollWithin.ts');
    expect(app).not.toContain('./scrollWithin.test.ts');
    expect(app.length).toBeGreaterThan(100);
  });

  it('is never called from the app', () => {
    const name = ['scroll', 'Into', 'View'].join('');
    expect(app.filter((path) => sources[path].includes(name))).toEqual([]);
  });
});
