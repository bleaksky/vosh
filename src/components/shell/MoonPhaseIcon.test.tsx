import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { moonPhaseShape } from '../../lib/moonPhase';
import { MoonPhaseIcon } from './MoonPhaseIcon';

function draw(phase: number | null): string {
  return renderToStaticMarkup(
    <MoonPhaseIcon phase={phase} color="#88c0d0" label="Nercuros, half-lit and growing" />,
  );
}

describe('MoonPhaseIcon', () => {
  it('draws at 14 px on the 16 unit grid with its label', () => {
    const svg = draw(2);
    expect(svg).toContain('width="14"');
    expect(svg).toContain('height="14"');
    expect(svg).toContain('viewBox="0 0 16 16"');
    expect(svg).toContain('role="img"');
    expect(svg).toContain('aria-label="Nercuros, half-lit and growing"');
    expect(svg).toContain('<title>Nercuros, half-lit and growing</title>');
  });

  it('shows the faint disc and outline under the lit part', () => {
    const svg = draw(3);
    expect(svg).toMatch(
      /<circle cx="8" cy="8" r="6.25" fill="#88c0d0" fill-opacity="0.22" stroke="#88c0d0" stroke-opacity="0.55" stroke-width="1.25"/,
    );
    const shape = moonPhaseShape(3);
    expect(svg).toContain(`<path d="${shape?.litPath}" fill="#88c0d0"`);
    expect(svg).toContain(
      `<path d="${shape?.litLimbPath}" fill="none" stroke="#88c0d0" stroke-width="1.25"`,
    );
    expect(svg.indexOf('<circle')).toBeLessThan(svg.indexOf('<path'));
  });

  it('lights nothing at new or without a phase', () => {
    expect(draw(0)).not.toContain('<path');
    expect(draw(null)).not.toContain('<path');
    expect(draw(0)).toContain('<circle');
  });
});
