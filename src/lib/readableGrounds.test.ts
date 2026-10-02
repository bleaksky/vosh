import { describe, expect, it } from 'vitest';
import fixture from '../../fixtures/readable/grounds.json';
import { PRESETS } from './presets';
import { BUILTIN_THEMES } from './themes';

// fixtures/readable/grounds.json feeds the Rust tests of Keep highlight
// colors readable (crates/trigger/src/readable.rs). These hold it to the
// themes and presets it was written from, so a theme or a preset color
// added later joins the Rust tests too.

const ANSI_NAMES = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'];

/** The text colors the presets paint, by kind. */
function presetColors() {
  const trueColor = new Set<string>();
  const indexed = new Set<number>();
  const named = new Set<string>();
  const hex = (n: number) => n.toString(16).padStart(2, '0');
  for (const preset of PRESETS) {
    for (const trigger of preset.triggers) {
      for (const action of trigger.actions) {
        if (action.kind === 'highlight' && action.style.fg) named.add(action.style.fg);
        if (action.kind !== 'replace') continue;
        // eslint-disable-next-line no-control-regex
        for (const m of action.template.matchAll(/\x1b\[([\d;]*)m/g)) {
          const codes = m[1].split(';').map(Number);
          for (let i = 0; i < codes.length; i++) {
            const code = codes[i];
            if (code === 38 && codes[i + 1] === 5) {
              indexed.add(codes[i + 2]);
              i += 2;
            } else if (code === 38 && codes[i + 1] === 2) {
              const [r, g, b] = codes.slice(i + 2, i + 5);
              trueColor.add(`#${hex(r)}${hex(g)}${hex(b)}`);
              i += 4;
            } else if (code >= 30 && code <= 37) {
              named.add(ANSI_NAMES[code - 30]);
            } else if (code >= 90 && code <= 97) {
              named.add(`bright_${ANSI_NAMES[code - 90]}`);
            }
          }
        }
      }
    }
  }
  return { trueColor, indexed, named };
}

describe('the readable grounds fixture', () => {
  it('holds every built in theme with its terminal background', () => {
    expect(fixture.grounds).toEqual(
      BUILTIN_THEMES.map((t) => ({ theme: t.id, background: t.xterm.background })),
    );
  });

  it('holds every color the presets paint text in', () => {
    const colors = presetColors();
    expect([...colors.indexed].sort((a, b) => a - b)).toEqual(fixture.colors.indexed);
    expect([...colors.named].sort()).toEqual(fixture.colors.named);
    for (const hex of colors.trueColor) expect(fixture.colors.true_color).toContain(hex);
  });

  it('holds the weather blue', () => {
    expect(fixture.colors.true_color).toContain('#8fa7d9');
  });
});
