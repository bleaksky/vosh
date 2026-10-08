// A stand in for the DOM, for tests that mount a page with React DOM
// in Node. It holds just what React DOM calls to mount and update a
// page. Events and layout are not here.

const ELEMENT_NODE = 1;
const TEXT_NODE = 3;
const DOCUMENT_NODE = 9;

export class FakeNode {
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

export class FakeElement extends FakeNode {
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
  /** React focuses a field with autoFocus when it mounts. */
  focus(): void {
    if (this.ownerDocument) this.ownerDocument.activeElement = this;
  }
  blur(): void {
    if (this.ownerDocument?.activeElement === this) this.ownerDocument.activeElement = null;
  }
  /** Itself or the nearest ancestor with a class that a list like
   *  `.a, .b` names. Class selectors alone. */
  closest(selectors: string): FakeElement | null {
    const wanted = selectors.split(',').map((s) => s.trim().replace(/^\./, ''));
    const has = (el: FakeElement) => {
      const classes = (el.getAttribute('class') ?? '').split(/\s+/);
      return wanted.some((c) => classes.includes(c));
    };
    if (has(this)) return this;
    for (let node = this.parentNode; node; node = node.parentNode) {
      if (node instanceof FakeElement && has(node)) return node;
    }
    return null;
  }
}

export class FakeDocument extends FakeNode {
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

export function findAll(root: FakeNode, match: (el: FakeElement) => boolean): FakeElement[] {
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
