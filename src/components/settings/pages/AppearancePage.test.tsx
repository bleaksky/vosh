import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { SystemFontEntry, UiConfig } from '../../../lib/session';
import type { AppearancePage as AppearancePageType } from './AppearancePage';

// The Font select waits on fonts_list, the one slow read on this page.
// The first read of a launch takes a moment, so the page must draw the
// gallery and the Font select without it and add the installed fonts
// when the list comes in. The page reads the list in an effect, so this
// test mounts it for real, into the small stand in for the DOM below.

const fonts = vi.hoisted(() => {
  let resolve: (list: SystemFontEntry[]) => void = () => {};
  const pending = new Promise<SystemFontEntry[]>((r) => {
    resolve = r;
  });
  return { pending, resolve: (list: SystemFontEntry[]) => resolve(list) };
});

const invoke = vi.hoisted(() =>
  vi.fn((cmd: string) => (cmd === 'fonts_list' ? fonts.pending : Promise.resolve(undefined))),
);

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// ── A stand in for the DOM ───────────────────────────────────────────
// Just what React DOM calls to mount and update this page. Events and
// layout are not here.

const ELEMENT_NODE = 1;
const TEXT_NODE = 3;
const DOCUMENT_NODE = 9;

class FakeNode {
  parentNode: FakeNode | null = null;
  childNodes: FakeNode[] = [];
  constructor(
    readonly nodeType: number,
    readonly nodeName: string,
    readonly ownerDocument: FakeDocument | null,
  ) {}
  get firstChild(): FakeNode | null {
    return this.childNodes[0] ?? null;
  }
  get lastChild(): FakeNode | null {
    return this.childNodes[this.childNodes.length - 1] ?? null;
  }
  appendChild(child: FakeNode): FakeNode {
    return this.insertBefore(child, null);
  }
  insertBefore(child: FakeNode, before: FakeNode | null): FakeNode {
    child.parentNode?.removeChild(child);
    const at = before ? this.childNodes.indexOf(before) : -1;
    if (at < 0) this.childNodes.push(child);
    else this.childNodes.splice(at, 0, child);
    child.parentNode = this;
    return child;
  }
  removeChild(child: FakeNode): FakeNode {
    this.childNodes = this.childNodes.filter((c) => c !== child);
    child.parentNode = null;
    return child;
  }
  get textContent(): string {
    return this.childNodes.map((c) => c.textContent).join('');
  }
  set textContent(text: string) {
    for (const child of this.childNodes) child.parentNode = null;
    this.childNodes = [];
    if (text && this.ownerDocument) this.appendChild(this.ownerDocument.createTextNode(text));
  }
  addEventListener(): void {}
  removeEventListener(): void {}
}

class FakeText extends FakeNode {
  constructor(
    public nodeValue: string,
    doc: FakeDocument,
  ) {
    super(TEXT_NODE, '#text', doc);
  }
  override get textContent(): string {
    return this.nodeValue;
  }
  override set textContent(text: string) {
    this.nodeValue = text;
  }
}

class FakeStyle {
  [name: string]: unknown;
  setProperty(name: string, value: string): void {
    this[name] = value;
  }
  removeProperty(name: string): void {
    delete this[name];
  }
}

class FakeElement extends FakeNode {
  readonly attributes = new Map<string, string>();
  readonly style = new FakeStyle();
  // A property set wins over the value attribute, as on an input.
  private valueProp: string | undefined;
  constructor(tag: string, doc: FakeDocument) {
    super(ELEMENT_NODE, tag.toUpperCase(), doc);
  }
  get tagName(): string {
    return this.nodeName;
  }
  setAttribute(name: string, value: string): void {
    this.attributes.set(name, String(value));
  }
  setAttributeNS(_ns: string | null, name: string, value: string): void {
    this.setAttribute(name, value);
  }
  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }
  hasAttribute(name: string): boolean {
    return this.attributes.has(name);
  }
  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }
  removeAttributeNS(_ns: string | null, name: string): void {
    this.removeAttribute(name);
  }
  get value(): string {
    return this.valueProp ?? this.getAttribute('value') ?? this.textContent;
  }
  set value(value: string) {
    this.valueProp = String(value);
  }
  /** A select's options, which React walks to mark the chosen one. */
  get options(): FakeElement[] {
    return findAll(this, (el) => el.nodeName === 'OPTION');
  }
}

class FakeDocument extends FakeNode {
  readonly body: FakeElement;
  readonly documentElement: FakeElement & { dataset: Record<string, string> };
  activeElement: FakeElement | null = null;
  constructor() {
    super(DOCUMENT_NODE, '#document', null);
    this.body = new FakeElement('body', this);
    this.documentElement = Object.assign(new FakeElement('html', this), { dataset: {} });
  }
  createElement(tag: string): FakeElement {
    return new FakeElement(tag, this);
  }
  createElementNS(_ns: string, tag: string): FakeElement {
    return new FakeElement(tag, this);
  }
  createTextNode(text: string): FakeText {
    return new FakeText(text, this);
  }
}

function findAll(root: FakeNode, match: (el: FakeElement) => boolean): FakeElement[] {
  const found: FakeElement[] = [];
  const walk = (node: FakeNode) => {
    for (const child of node.childNodes) {
      if (child instanceof FakeElement) {
        if (match(child)) found.push(child);
        walk(child);
      }
    }
  };
  walk(root);
  return found;
}

// ── The page ─────────────────────────────────────────────────────────

const doc = new FakeDocument();
let AppearancePage: typeof AppearancePageType;
let createRoot: typeof import('react-dom/client').createRoot;
let normalizeUiConfig: typeof import('../../../lib/session').normalizeUiConfig;
let BUILTIN_THEMES: typeof import('../../../lib/themes').BUILTIN_THEMES;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    matchMedia: () => ({ matches: true, addEventListener() {}, removeEventListener() {} }),
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  // React DOM reads navigator.userAgent when it loads. Node 20, the CI
  // version, has no navigator of its own.
  vi.stubGlobal('navigator', { userAgent: 'node' });
  // React DOM checks for a DOM once, when it loads, so it loads now.
  ({ createRoot } = await import('react-dom/client'));
  ({ AppearancePage } = await import('./AppearancePage'));
  ({ normalizeUiConfig } = await import('../../../lib/session'));
  ({ BUILTIN_THEMES } = await import('../../../lib/themes'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const CURRENT = '"PT Mono", Menlo, monospace';

function config(): UiConfig {
  return normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: CURRENT,
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  });
}

/** The Font select's options as label and value. */
function fontOptions(root: FakeNode): { label: string; value: string }[] {
  const [row] = findAll(root, (el) => el.getAttribute('data-st-anchor') === 'font');
  const [select] = findAll(row, (el) => el.nodeName === 'SELECT');
  return select.options.map((o) => ({ label: o.textContent, value: o.value }));
}

describe('AppearancePage', () => {
  it('draws the gallery and the Font select before fonts_list answers', async () => {
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    const props = {
      target: { group: 'appearance' },
      navSeq: 0,
      config: config(),
      setConfig: () => undefined,
      onError: () => undefined,
      pathB: false,
      navigate: () => undefined,
      setLeaveGuard: () => undefined,
    } as const;

    await act(async () => {
      root.render(createElement(AppearancePage, props));
    });

    // fonts_list is still out.
    expect(invoke).toHaveBeenCalledWith('fonts_list');
    const radios = findAll(container, (el) => el.getAttribute('type') === 'radio');
    expect(radios.length).toBe(BUILTIN_THEMES.length);
    expect(fontOptions(container)).toEqual([
      { label: 'PT Mono', value: CURRENT },
      { label: 'Berkeley Mono', value: '"BerkeleyMono Bundled", Menlo, monospace' },
      { label: 'JetBrains Mono', value: '"JetBrainsMono Bundled", Menlo, monospace' },
    ]);

    await act(async () => {
      fonts.resolve([
        { family: 'Helvetica', monospace: false },
        { family: 'JetBrains Mono', monospace: true },
        { family: 'Menlo', monospace: true },
        { family: 'PT Mono', monospace: true },
        { family: 'Times', monospace: false },
      ]);
      await fonts.pending;
    });

    // The monospace families join the bundled ones. The proportional
    // ones stay out, and your font keeps its exact list.
    expect(fontOptions(container)).toEqual([
      { label: 'Berkeley Mono', value: '"BerkeleyMono Bundled", Menlo, monospace' },
      { label: 'JetBrains Mono', value: '"JetBrainsMono Bundled", Menlo, monospace' },
      { label: 'Menlo', value: '"Menlo", Menlo, monospace' },
      { label: 'PT Mono', value: CURRENT },
    ]);
    expect(findAll(container, (el) => el.getAttribute('type') === 'radio').length).toBe(
      BUILTIN_THEMES.length,
    );

    await act(async () => {
      root.unmount();
    });
  });
});
