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

describe('MoonPhaseIcon on a light theme', () => {
  function ink(phase: number | null): string {
    return renderToStaticMarkup(
      <MoonPhaseIcon phase={phase} color="#3b3632" label="Lysenties" onLight />,
    );
  }

  it('fills the dark part and leaves the lit part as paper', () => {
    const svg = ink(2);
    const shape = moonPhaseShape(2);
    // The mask keeps the disc and cuts the lit part out of it.
    expect(svg).toMatch(/<mask id="(moon-[\w-]+)">/);
    const id = /<mask id="(moon-[\w-]+)">/.exec(svg)?.[1];
    expect(svg).toContain(`<path d="${shape?.litPath}" fill="black"`);
    expect(svg).toContain(`fill="#3b3632" mask="url(#${id})"`);
    // The whole limb carries a full outline, with no faint fill.
    expect(svg).toMatch(/fill="none" stroke="#3b3632" stroke-width="1.25"/);
    expect(svg).not.toContain('fill-opacity');
  });

  it('draws new as a solid disc and full as an open ring', () => {
    // New lights nothing, so the mask cuts nothing and the disc fills.
    const fresh = ink(0);
    expect(fresh).toContain('<mask');
    expect(fresh).not.toContain('fill="black"');
    // Full lights the whole disc, so the mask cuts all of it away.
    const full = ink(4);
    expect(full).toContain(`<path d="${moonPhaseShape(4)?.litPath}" fill="black"`);
  });

  it('keeps each mask id to itself', () => {
    const two = renderToStaticMarkup(
      <>
        <MoonPhaseIcon phase={2} color="#000" label="a" onLight />
        <MoonPhaseIcon phase={6} color="#000" label="b" onLight />
      </>,
    );
    const ids = [...two.matchAll(/<mask id="([^"]+)">/g)].map((m) => m[1]);
    expect(ids).toHaveLength(2);
    expect(new Set(ids).size).toBe(2);
  });
});
