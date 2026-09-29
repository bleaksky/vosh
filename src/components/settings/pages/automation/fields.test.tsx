import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { Row } from '../../ui';
import { NumberField } from './fields';

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
