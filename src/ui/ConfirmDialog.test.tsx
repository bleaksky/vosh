import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ConfirmDialog } from './ConfirmDialog';

// The confirm card as markup. Effects do not run here, so neither the
// focus trap nor the escape stack starts.

const none = () => undefined;

/** The card's buttons, each with its class and whether it is off. */
function buttons(html: string) {
  return [...html.matchAll(/<button type="button"( disabled="")? class="([^"]*)">([^<]*)</g)].map(
    ([, off, cls, label]) => ({ cls, off: off !== undefined, label }),
  );
}

describe('ConfirmDialog', () => {
  it('confirms in the danger tone when no tone is named, as every older caller does', () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        title="Delete Healer?"
        body="Vosh deletes its file."
        confirmLabel="Delete"
        onConfirm={none}
        onCancel={none}
      />,
    );
    expect(buttons(html)).toEqual([
      { cls: 'btn', off: false, label: 'Cancel' },
      { cls: 'btn is-danger', off: false, label: 'Delete' },
    ]);
  });

  it('fills the confirm with the accent in the primary tone, and holds it off', () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        title="New plugin"
        body="Vosh makes a folder for it in your plugins folder and opens it here."
        confirmLabel="Create"
        tone="primary"
        confirmDisabled
        onConfirm={none}
        onCancel={none}
      />,
    );
    expect(buttons(html)).toEqual([
      { cls: 'btn', off: false, label: 'Cancel' },
      { cls: 'btn is-primary', off: true, label: 'Create' },
    ]);
  });

  it('draws its fields between the body and the buttons', () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        title="New plugin"
        body="Vosh makes a folder for it in your plugins folder and opens it here."
        confirmLabel="Create"
        tone="primary"
        onConfirm={none}
        onCancel={none}
      >
        <label className="ov-field-label">Name</label>
        <p className="ov-hint">Letters, digits, and underscores.</p>
      </ConfirmDialog>,
    );
    const order = [
      'class="ov-confirm-body"',
      'class="ov-field-label"',
      'class="ov-hint"',
      'class="ov-confirm-actions"',
    ].map((mark) => html.indexOf(mark));
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
  });

  it('names its cancel button when you pass a label', () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        title="Let Vosh post banners?"
        body="Vosh posts banners only for the alerts you turn on. macOS asks you next."
        confirmLabel="Continue"
        cancelLabel="Not now"
        tone="primary"
        onConfirm={none}
        onCancel={none}
      />,
    );
    expect(buttons(html)).toEqual([
      { cls: 'btn', off: false, label: 'Not now' },
      { cls: 'btn is-primary', off: false, label: 'Continue' },
    ]);
  });
});
