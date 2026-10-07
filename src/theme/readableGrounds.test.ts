import { describe, expect, it } from 'vitest';
import fixture from '../../fixtures/readable/grounds.json';
import { PRESETS, presetTriggers } from '../automation/presets';
import { BUILTIN_THEMES } from './themes';

// fixtures/readable/grounds.json feeds the Rust tests of Keep highlight
// colors readable (crates/automation/src/trigger/readable.rs). These hold
// it to the themes and presets it was written from, so a theme, a preset
// color or a preset template added later joins the Rust tests too.

/** Every Replace template of the presets once, in the order they list them. */
function presetTemplates(): string[] {
  const templates: string[] = [];
  for (const preset of PRESETS) {
    for (const trigger of presetTriggers(preset)) {
      for (const action of trigger.actions) {
        if (action.kind === 'replace' && !templates.includes(action.template)) {
          templates.push(action.template);
        }
      }
    }
  }
  return templates;
}

/** The fixed text colors the preset templates paint, by kind. */
function presetColors() {
  const trueColor = new Set<string>();
  const indexed = new Set<number>();
  const hex = (n: number) => n.toString(16).padStart(2, '0');
  for (const template of presetTemplates()) {
    // eslint-disable-next-line no-control-regex
    for (const m of template.matchAll(/\x1b\[([\d;]*)m/g)) {
      const codes = m[1].split(';').map(Number);
      for (let i = 0; i < codes.length; i++) {
        if (codes[i] === 38 && codes[i + 1] === 5) {
          indexed.add(codes[i + 2]);
          i += 2;
        } else if (codes[i] === 38 && codes[i + 1] === 2) {
          const [r, g, b] = codes.slice(i + 2, i + 5);
          trueColor.add(`#${hex(r)}${hex(g)}${hex(b)}`);
          i += 4;
        }
      }
    }
  }
  return { trueColor, indexed };
}

describe('the readable grounds fixture', () => {
  it('holds every built in theme with its terminal background', () => {
    expect(fixture.grounds).toEqual(
      BUILTIN_THEMES.map((t) => ({ theme: t.id, background: t.xterm.background })),
    );
  });

  it('holds every Replace template of the presets', () => {
    expect(fixture.templates).toEqual(presetTemplates());
  });

  it('holds every fixed color the presets paint text in', () => {
    const colors = presetColors();
    expect([...colors.indexed].sort((a, b) => a - b)).toEqual(fixture.colors.indexed);
    for (const hex of colors.trueColor) expect(fixture.colors.true_color).toContain(hex);
  });

  it('holds only indexes past the 16, which draw the same on every theme', () => {
    for (const n of fixture.colors.indexed) {
      expect(n).toBeGreaterThanOrEqual(16);
      expect(n).toBeLessThanOrEqual(255);
    }
  });

  it('holds the weather blue', () => {
    expect(fixture.colors.true_color).toContain('#8fa7d9');
  });
});
