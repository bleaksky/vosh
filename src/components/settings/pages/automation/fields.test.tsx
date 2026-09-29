import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { Row } from '../../ui';
import { CodeRow, NumberField } from './fields';

// CodeMirror draws nothing on the server, so a stand in shows the props
// CodeRow hands the editor.
vi.mock('../../../CodeEditor', async () => {
  const { createElement } = await import('react');
  return {
    CodeEditor: (props: Record<string, unknown>) =>
      createElement('div', {
        'data-props': JSON.stringify(props, (_key, value: unknown) =>
          typeof value === 'function' ? undefined : value,
        ),
      }),
  };
});

/** The value of `name` on the first tag that matches `tag`. */
function attr(html: string, tag: RegExp, name: string): string | null {
  const found = tag.exec(html);
  if (!found) return null;
  const value = new RegExp(`\\s${name}="([^"]*)"`).exec(found[0]);
  return value ? value[1] : null;
}

describe('NumberField', () => {
  it('describes the field by its unit', () => {
    const html = renderToStaticMarkup(
      <Row label="Every">
        <NumberField value={30} unit="seconds" onChange={() => {}} />
      </Row>,
    );
    const unitId = attr(html, /<span[^>]*class="st-auto-unit"[^>]*>/, 'id');
    expect(unitId).toBeTruthy();
    expect(html).toContain(`id="${unitId}" class="st-auto-unit">seconds</span>`);
    expect(attr(html, /<input[^>]*>/, 'aria-describedby')).toBe(unitId);
  });

  it('keeps the row description and adds the unit after it', () => {
    const html = renderToStaticMarkup(
      <Row label="Warn at" description="Before the tick fires.">
        <NumberField value={5} unit="seconds left" onChange={() => {}} />
      </Row>,
    );
    const descId = attr(html, /<span[^>]*class="st-row-desc"[^>]*>/, 'id');
    const unitId = attr(html, /<span[^>]*class="st-auto-unit"[^>]*>/, 'id');
    expect(attr(html, /<input[^>]*>/, 'aria-describedby')).toBe(`${descId} ${unitId}`);
  });

  it('draws no unit when it has none', () => {
    const html = renderToStaticMarkup(
      <Row label="Priority">
        <NumberField value={5} onChange={() => {}} />
      </Row>,
    );
    expect(html).not.toContain('st-auto-unit');
    expect(attr(html, /<input[^>]*>/, 'aria-describedby')).toBeNull();
  });
});

describe('CodeRow', () => {
  /** The props the editor got. */
  function editorProps(html: string): Record<string, unknown> {
    const raw = attr(html, /<div data-props="[^"]*"[^>]*>/, 'data-props') ?? '{}';
    const json = raw.replace(/&quot;/g, '"').replace(/&amp;/g, '&');
    return JSON.parse(json) as Record<string, unknown>;
  }

  it('names the editor by its row label and describes it by the row description', () => {
    const html = renderToStaticMarkup(
      <CodeRow label="Lua script" description="Runs on each match." value="" onChange={() => {}} />,
    );
    const labelId = attr(html, /<span[^>]*class="st-row-label"[^>]*>/, 'id');
    const descId = attr(html, /<span[^>]*class="st-row-desc"[^>]*>/, 'id');
    expect(labelId).toBeTruthy();
    expect(descId).toBeTruthy();
    const props = editorProps(html);
    expect(props.ariaLabelledBy).toBe(labelId);
    expect(props.ariaDescribedBy).toBe(descId);
  });

  it('leaves out the description when the row has none', () => {
    const html = renderToStaticMarkup(<CodeRow label="Lua script" value="" onChange={() => {}} />);
    expect(editorProps(html).ariaDescribedBy).toBeUndefined();
  });
});
