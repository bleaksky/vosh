import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { Disclosure } from './Disclosure';

// The Advanced row reads its label as its name and the line under it,
// with the count of edits when there is one, as its description
// (Board 12).

// This test DOM has no parser, so it reads the markup. Each id names a
// span or a paragraph whose text holds no further spans.
function parse(html: string) {
  const button = /<button[^>]*>/.exec(html)![0];
  const text = (attr: string) =>
    (new RegExp(`${attr}="([^"]*)"`).exec(button)?.[1] ?? '')
      .split(' ')
      .filter(Boolean)
      .map((id) => {
        const inner = new RegExp(`id="${id}"[^>]*>(.*?)</(span|p)>`).exec(html)?.[1] ?? '';
        return inner.replace(/<[^>]*>/g, '');
      });
  return {
    describedBy: button.includes('aria-describedby='),
    name: text('aria-labelledby'),
    description: text('aria-describedby'),
  };
}

describe('a Disclosure', () => {
  it('is named by its label and described by the line under it', () => {
    const { name, description } = parse(
      renderToStaticMarkup(
        <Disclosure
          label="Advanced"
          description="Set priority, match prompts, send to a pane, run Lua, or tune alerts."
          expanded={false}
        />,
      ),
    );
    expect(name).toEqual(['Advanced']);
    expect(description).toEqual([
      'Set priority, match prompts, send to a pane, run Lua, or tune alerts.',
    ]);
  });

  it('reads the note after the description', () => {
    const { name, description } = parse(
      renderToStaticMarkup(
        <Disclosure
          label="Advanced"
          description="Set priority, match prompts, send to a pane, run Lua, or tune alerts."
          note={<span className="st-auto-count">1 change</span>}
          expanded={false}
        />,
      ),
    );
    expect(name).toEqual(['Advanced']);
    expect(description).toEqual([
      'Set priority, match prompts, send to a pane, run Lua, or tune alerts.',
      '1 change',
    ]);
  });

  it('has no description without a line or a note', () => {
    const { describedBy, name } = parse(
      renderToStaticMarkup(<Disclosure label="Advanced" expanded />),
    );
    expect(name).toEqual(['Advanced']);
    expect(describedBy).toBe(false);
  });

  it('keeps a description the caller passes, after its own', () => {
    const html = renderToStaticMarkup(
      <>
        <Disclosure
          label="Advanced"
          description="More rows."
          expanded={false}
          aria-describedby="hint"
        />
        <p id="hint">Saved for this profile.</p>
      </>,
    );
    expect(parse(html).description).toEqual(['More rows.', 'Saved for this profile.']);
  });
});
