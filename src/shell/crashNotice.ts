// The notice an uncaught error or rejection paints into the page. It names
// a failure the webview would otherwise swallow, since WKWebView gives no
// console to read. At boot nothing else may render, so it takes the top
// of the window. Once the app has mounted it sits small in the top right
// corner, below the title band, clear of the command line. Errors that
// follow join it with a count rather than stacking, and Close or Escape
// takes it away.
//
// It draws with inline styles and the theme's tokens where they loaded,
// since a boot failure can land before the stylesheets do.

/** The most error texts a notice keeps for Copy details. */
const KEPT = 20;

/** Where the notice sits: the top of the window at boot, the corner
 *  after the app mounted. */
const PLACE = {
  boot: 'top:12px;left:12px;right:12px;max-height:60vh;',
  corner: 'top:44px;right:12px;width:min(420px, calc(100vw - 24px));max-height:40vh;',
};

const BOX =
  'position:fixed;z-index:99999;display:flex;flex-direction:column;gap:8px;' +
  'box-sizing:border-box;padding:10px 12px;border-radius:12px;' +
  'background:var(--raised, #1b1a19);color:var(--text, #e6e2df);' +
  'border:1px solid var(--danger, #ea8f80);box-shadow:0 8px 24px rgba(0,0,0,0.35);' +
  'font:13px/18px var(--font-ui, system-ui, sans-serif);';
const HEAD = 'display:flex;align-items:center;gap:8px;';
const TITLE = 'flex:1;min-width:0;font-weight:600;color:var(--danger-text, #ea8f80);';
const NOTE = 'margin:0;color:var(--secondary, #b5b0ad);';
const DETAIL =
  'margin:0;min-height:0;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;' +
  'font:11px/15px ui-monospace, monospace;color:var(--secondary, #b5b0ad);';
const BUTTON =
  'flex:none;padding:2px 10px;border-radius:8px;border:1px solid var(--sep, #3a3836);' +
  'background:transparent;color:var(--text, #e6e2df);font:inherit;cursor:pointer;';

/** One line of text for an error or a rejection reason. */
export function errorText(label: string, detail: unknown): string {
  const err = detail instanceof Error ? `${detail.message}\n${detail.stack ?? ''}` : String(detail);
  return `${label}: ${err}`.trimEnd();
}

export interface CrashNotice {
  /** Show `detail` under `label`, or count it into the notice that shows. */
  show(label: string, detail: unknown): void;
  /** Take the notice away. */
  close(): void;
}

/** The crash notice of `doc`. `copy` puts text on the clipboard. */
export function crashNotice(
  doc: Document,
  copy: (text: string) => Promise<void> = (text) => navigator.clipboard.writeText(text),
): CrashNotice {
  let box: HTMLElement | null = null;
  let title: HTMLElement | null = null;
  let note: HTMLElement | null = null;
  let detail: HTMLElement | null = null;
  let copyButton: HTMLElement | null = null;
  const texts: string[] = [];
  let count = 0;

  // The app has mounted once its root holds anything.
  const mounted = () => (doc.getElementById('root')?.childElementCount ?? 0) > 0;

  const onKey = (event: KeyboardEvent) => {
    if (event.key === 'Escape') close();
  };

  function close(): void {
    box?.remove();
    box = title = note = detail = copyButton = null;
    texts.length = 0;
    count = 0;
    doc.removeEventListener('keydown', onKey, true);
  }

  const button = (label: string, onClick: () => void): HTMLElement => {
    const el = doc.createElement('button');
    el.setAttribute('type', 'button');
    el.style.cssText = BUTTON;
    el.textContent = label;
    el.addEventListener('click', onClick);
    return el;
  };

  function build(): void {
    box = doc.createElement('div');
    box.setAttribute('role', 'alert');
    const head = doc.createElement('div');
    head.style.cssText = HEAD;
    title = doc.createElement('div');
    title.style.cssText = TITLE;
    copyButton = button('Copy details', () => {
      const done = (label: string) => {
        if (copyButton) copyButton.textContent = label;
      };
      copy(texts.join('\n\n')).then(
        () => done('Copied'),
        () => done('Copy failed'),
      );
    });
    head.append(title, copyButton, button('Close', close));
    note = doc.createElement('p');
    note.style.cssText = NOTE;
    detail = doc.createElement('pre');
    detail.style.cssText = DETAIL;
    box.append(head, note, detail);
    doc.body.appendChild(box);
    doc.addEventListener('keydown', onKey, true);
  }

  function show(label: string, err: unknown): void {
    try {
      const text = errorText(label, err);
      if (!box?.isConnected) {
        close();
        build();
      }
      count += 1;
      texts.push(text);
      if (texts.length > KEPT) texts.shift();
      const atBoot = !mounted();
      if (box) box.style.cssText = BOX + (atBoot ? PLACE.boot : PLACE.corner);
      if (title) {
        title.textContent =
          count > 1 ? `Something went wrong ${count} times` : 'Something went wrong';
      }
      if (note) {
        note.textContent = atBoot
          ? 'Vosh may not have started. Restart it, and copy the details if you want to report this.'
          : 'You can keep playing. Copy the details if you want to report this.';
      }
      if (detail) detail.textContent = text;
      if (copyButton) copyButton.textContent = 'Copy details';
    } catch {
      // The notice must never throw.
    }
  }

  return { show, close };
}
