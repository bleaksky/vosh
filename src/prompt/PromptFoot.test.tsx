import { act, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { PromptShowState } from '../ipc/prompt';
import type { PromptPreviewName } from '../ipc/promptDesign';
import { findAll, type FakeElement } from '../test/fakeDom';
import {
  BUTTON,
  checkMarks,
  menuButtonDom,
  menuHeight,
  MENU_WIDTH,
  on,
} from '../test/menuButtonDom';
import { DesignFoot, PreviewButton } from './PromptFoot';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The foot under your design in Customize prompt, and the Preview menu
// button on its right, before Done.

const reads: PromptShowState = {
  show: 'pinned',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

const LABELS: Record<PromptPreviewName, string> = {
  now: 'Now',
  low_health: 'Low health',
  fight: 'Fight',
  lament: 'Lament',
};

function foot(draw: boolean, onPreview = vi.fn()): ReactElement {
  return (
    <DesignFoot
      draw={draw}
      onDraw={() => {}}
      show="pinned"
      showState={reads}
      onShow={() => {}}
      preview="fight"
      forsaken
      onPreview={onPreview}
      onDone={() => {}}
    />
  );
}

describe('the Preview button', () => {
  const draw = (value: PromptPreviewName, forsaken = true) =>
    renderToStaticMarkup(<PreviewButton value={value} forsaken={forsaken} onChange={() => {}} />);

  it('reads Preview: and the preview now, with a chevron', () => {
    for (const [value, label] of Object.entries(LABELS) as [PromptPreviewName, string][]) {
      const html = draw(value);
      expect(html, value).toMatch(
        new RegExp(
          `<button[^>]*class="st-button st-button-secondary pc-menu-button"[^>]*>` +
            `<span><span class="pc-menu-button-lead">Preview: </span>${label}</span><svg`,
        ),
      );
      // A screen reader hears Preview and the preview now.
      expect(html, value).toContain(`aria-label="Preview, ${label}"`);
      expect(html, value).toContain('aria-haspopup="menu"');
      expect(html, value).toContain('aria-expanded="false"');
      // Never off, and the menu waits for a press.
      expect(html, value).not.toContain('aria-disabled');
      expect(html, value).not.toContain('title=');
      expect(html, value).not.toContain('role="menu"');
    }
  });

  it('is the one kind of button the place of your prompt uses', () => {
    const preview = draw('now');
    const place = renderToStaticMarkup(foot(false));
    const button = (html: string) => /<button[^>]*aria-haspopup="menu"[^>]*>/.exec(html)?.[0];
    expect(button(preview)).toContain('class="st-button st-button-secondary pc-menu-button"');
    expect(button(place)).toContain('class="st-button st-button-secondary pc-menu-button"');
  });
});

describe('the foot under your design', () => {
  it('reads Draw your prompt, the place, the preview and Done on one row while drawing is on', () => {
    const html = renderToStaticMarkup(foot(true));
    // The one row of every foot, no class that lets it wrap.
    expect(html).toMatch(/^<div class="pc-foot">/);
    const at = (text: string) => {
      const i = html.indexOf(text);
      expect(i, text).toBeGreaterThanOrEqual(0);
      return i;
    };
    const order = [
      at('role="switch"'),
      at('>Draw your prompt</label>'),
      at('aria-label="Where your prompt shows, Pinned"'),
      at('<div class="pc-foot-end">'),
      at('aria-label="Preview, Fight"'),
      at('>Done</button>'),
    ];
    expect(order).toEqual([...order].sort((a, b) => a - b));
    // The segments gave way to the menu.
    expect(html).not.toContain('st-seg');
    expect(html).not.toContain('role="group"');
  });

  it('drops the preview while drawing is off, and keeps the row and Done', () => {
    const html = renderToStaticMarkup(foot(false));
    expect(html).toMatch(/^<div class="pc-foot">/);
    expect(html).not.toContain('Preview');
    expect(html).toContain('aria-label="Where your prompt shows, Pinned"');
    expect(html).toMatch(/<div class="pc-foot-end"><button[^>]*>Done<\/button><\/div>/);
  });
});

// ── The menu, mounted ───────────────────────────────────────────────
// React DOM mounts the button on the stand in DOM with the card's own
// menu (src/test/menuButtonDom.ts).

const isPreview = (el: FakeElement) =>
  el.getAttribute('aria-label')?.startsWith('Preview') ?? false;

describe('the Preview menu', () => {
  const { doc, mount: mountElement } = menuButtonDom();

  const mount = async (value: PromptPreviewName = 'now', forsaken = false) => {
    const onChange = vi.fn();
    const m = await mountElement(
      <PreviewButton value={value} forsaken={forsaken} onChange={onChange} />,
    );
    return { ...m, onChange };
  };

  it('opens on a press with the previews, the current one checked on the right', async () => {
    const m = await mount('low_health');
    // A plain button, so Enter and Space press it as they press any.
    expect(m.button.getAttribute('type')).toBe('button');
    expect(on(m.button).onKeyDown).toBeUndefined();
    expect(m.menu()).toBeNull();

    await m.press();
    const menu = m.menu();
    expect(menu).not.toBeNull();
    expect(menu?.getAttribute('aria-label')).toBe('Preview');
    expect(menu?.getAttribute('class')).toBe('menu');
    expect(m.button.getAttribute('aria-expanded')).toBe('true');
    expect(m.items().map((el) => el.textContent)).toEqual(['Now', 'Low health', 'Fight']);
    for (const item of m.items()) {
      expect(item.getAttribute('class')).toBe('menu-item');
      expect(item.getAttribute('role')).toBe('menuitemradio');
      expect(checkMarks(item), item.textContent ?? '').toBe(
        item.textContent === 'Low health' ? 1 : 0,
      );
    }
    expect(m.checked()).toEqual(['Low health']);
    // It opens above the button, their right edges together, as wide
    // as its rows ask, and takes focus.
    expect(menu?.style.left).toBe(`${BUTTON.right - MENU_WIDTH}px`);
    expect(menu?.style.top).toBe(`${BUTTON.top - 4 - menuHeight(3)}px`);
    expect(doc.activeElement).toBe(menu);

    // A second press shuts it.
    await m.press();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('offers Lament under the Forsaken Lands rules', async () => {
    const m = await mount('lament', true);
    expect(m.button.getAttribute('aria-label')).toBe('Preview, Lament');
    await m.press();
    expect(m.items().map((el) => el.textContent)).toEqual(['Now', 'Low health', 'Fight', 'Lament']);
    expect(m.checked()).toEqual(['Lament']);
    expect(m.menu()?.style.top).toBe(`${BUTTON.top - 4 - menuHeight(4)}px`);
  });

  it('moves between the previews with the arrow keys, and closes on Esc', async () => {
    const m = await mount('fight');
    await m.press();
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('Now'));
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('Low health'));
    await m.key('ArrowUp');
    expect(doc.activeElement).toBe(m.item('Now'));
    // Past either end it comes round.
    await m.key('ArrowUp');
    expect(doc.activeElement).toBe(m.item('Fight'));
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('Now'));
    await m.key('End');
    expect(doc.activeElement).toBe(m.item('Fight'));
    await m.key('Home');
    expect(doc.activeElement).toBe(m.item('Now'));

    await m.escape();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('closes on Tab or a press outside, leaving the preview as it was', async () => {
    const m = await mount('now');
    await m.press();
    await m.key('ArrowDown');
    await m.key('Tab');
    expect(m.menu()).toBeNull();
    await m.press();
    await m.pressOutside();
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('shows the preview you pick and hands focus back to the button', async () => {
    const m = await mount('now');
    await m.press();
    await m.key('End');
    const fight = m.item('Fight');
    expect(doc.activeElement).toBe(fight);
    // Enter and Space press the item, as a click does.
    await act(async () => on(fight).onClick());
    expect(m.onChange).toHaveBeenCalledTimes(1);
    expect(m.onChange).toHaveBeenCalledWith('fight');
    expect(m.menu()).toBeNull();
    expect(doc.activeElement).toBe(m.button);
  });

  it('closes without a change when you pick the preview it shows now', async () => {
    const m = await mount('now');
    await m.press();
    await act(async () => on(m.item('Now')).onClick());
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
    expect(doc.activeElement).toBe(m.button);
  });

  it('goes with its open menu when drawing turns off', async () => {
    const onPreview = vi.fn();
    const m = await mountElement(foot(true, onPreview), isPreview);
    await m.press();
    expect(m.menu()?.getAttribute('aria-label')).toBe('Preview');
    await m.update(foot(false, onPreview));
    expect(m.menu()).toBeNull();
    expect(findAll(m.container, isPreview).length).toBe(0);
    expect(
      findAll(m.container, (el) => el.nodeName === 'BUTTON' && el.textContent === 'Done').length,
    ).toBe(1);
    expect(onPreview).not.toHaveBeenCalled();
  });
});
