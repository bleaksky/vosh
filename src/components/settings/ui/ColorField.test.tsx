import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ColorField } from './ColorField';

function draw(value: string, hexOnly: boolean): string {
  return renderToStaticMarkup(
    <ColorField value={value} onChange={() => undefined} allowEmpty hexOnly={hexOnly} />,
  );
}

describe('ColorField', () => {
  it('marks a saved color a hex only field cannot use', () => {
    // An older build saved any CSS color, and the terminal ignored it.
    const html = draw('red', true);
    expect(html).toContain('data-invalid=""');
    expect(html).toContain('aria-invalid="true"');
    expect(html).toContain('Type a hex color like #88c0d0.');
  });

  it('leaves a hex color and an empty field alone', () => {
    for (const value of ['#88c0d0', '']) {
      const html = draw(value, true);
      expect(html).not.toContain('data-invalid');
      expect(html).not.toContain('aria-invalid');
    }
  });

  it('takes any CSS color when it is not hex only', () => {
    const html = draw('red', false);
    expect(html).not.toContain('data-invalid');
    expect(html).not.toContain('aria-invalid');
  });
});
