import { act } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { PromptShow, PromptShowState } from '../ipc/prompt';
import { BUTTON, checkMarks, menuButtonDom, menuHeight, on } from '../test/menuButtonDom';
import { ShowButton } from './PromptShow';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// The button beside Draw your prompt at the foot of Customize prompt,
// and the menu it opens.

const reads: PromptShowState = {
  show: 'pinned',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

describe('the button that says where your prompt shows', () => {
  const draw = (value: PromptShow, state: PromptShowState | null = reads) =>
    renderToStaticMarkup(<ShowButton value={value} state={state} onChange={() => {}} />);

  it('reads the place your prompt shows now, with a chevron', () => {
    for (const [value, label] of [
      ['text', 'In the text'],
      ['lifted', 'Lifted'],
      ['pinned', 'Pinned'],
    ] as const) {
      const html = draw(value);
      expect(html, value).toMatch(
        new RegExp(`<button[^>]*class="btn pc-menu-button"[^>]*><span>${label}</span><svg`),
      );
      // A screen reader hears what the button picks and the place now.
      expect(html, value).toContain(`aria-label="Where your prompt shows, ${label}"`);
      expect(html, value).toContain('aria-haspopup="menu"');
      expect(html, value).toContain('aria-expanded="false"');
      expect(html, value).not.toContain('aria-disabled');
      expect(html, value).not.toContain('title=');
      // The menu waits for a press.
      expect(html, value).not.toContain('role="menu"');
    }
  });

  it('turns off and says why while the profile reads no prompt, as the Settings row does', () => {
    for (const [gameSent, why] of [
      [true, 'Customize your prompt first.'],
      [false, 'Tell Vosh your game&#x27;s prompt first.'],
    ] as const) {
      const html = draw('text', { ...reads, capture: false, gameSent });
      expect(html).toContain('aria-disabled="true"');
      expect(html).toContain(`title="${why}"`);
      const id = /aria-describedby="([^"]+)"/.exec(html)?.[1];
      expect(id).toBeTruthy();
      expect(html).toContain(`<span id="${id}" class="visually-hidden">${why}</span>`);
      // Off, not gone, so Tab still reaches it and a reader hears why.
      expect(html).not.toMatch(/<button[^>]*disabled=""/);
    }
  });

  it('waits, off with nothing to say, until Vosh knows whether the profile reads a prompt', () => {
    const html = draw('pinned', null);
    expect(html).toContain('aria-disabled="true"');
    expect(html).not.toContain('aria-describedby');
    expect(html).not.toContain('title=');
    expect(html).toContain('<span>Pinned</span>');
  });
});

// ── The menu, mounted ───────────────────────────────────────────────
// React DOM mounts the button on the stand in DOM with the card's own
// menu (src/test/menuButtonDom.ts).

describe('the menu of where your prompt shows', () => {
  const { doc, mount: mountElement } = menuButtonDom();

  const mount = async (value: PromptShow = 'pinned', state: PromptShowState | null = reads) => {
    const onChange = vi.fn();
    const draw = (now: PromptShowState | null) => (
      <ShowButton value={value} state={now} onChange={onChange} />
    );
    const m = await mountElement(draw(state));
    return { ...m, onChange, update: (now: PromptShowState | null) => m.update(draw(now)) };
  };

  it('opens on a press with the three places, the current one checked on the right', async () => {
    const m = await mount('lifted');
    // A plain button, so Enter and Space press it as they press any.
    expect(m.button.getAttribute('type')).toBe('button');
    expect(on(m.button).onKeyDown).toBeUndefined();
    expect(m.menu()).toBeNull();

    await m.press();
    const menu = m.menu();
    expect(menu).not.toBeNull();
    expect(menu?.getAttribute('aria-label')).toBe('Where your prompt shows');
    expect(menu?.getAttribute('class')).toBe('menu');
    expect(m.button.getAttribute('aria-expanded')).toBe('true');
    expect(m.items().map((el) => el.textContent)).toEqual(['In the text', 'Lifted', 'Pinned']);
    for (const item of m.items()) {
      expect(item.getAttribute('class')).toBe('menu-item');
      expect(checkMarks(item), item.textContent ?? '').toBe(item.textContent === 'Lifted' ? 1 : 0);
    }
    expect(m.checked()).toEqual(['Lifted']);
    // It opens above the button, their left edges together, and takes
    // focus.
    expect(menu?.style.left).toBe(`${BUTTON.left}px`);
    expect(menu?.style.top).toBe(`${BUTTON.top - 4 - menuHeight(3)}px`);
    expect(doc.activeElement).toBe(menu);

    // A second press shuts it.
    await m.press();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('moves between the places with the arrow keys, and closes on Esc', async () => {
    const m = await mount('pinned');
    await m.press();
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('In the text'));
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('Lifted'));
    await m.key('ArrowUp');
    expect(doc.activeElement).toBe(m.item('In the text'));
    // Past either end it comes round.
    await m.key('ArrowUp');
    expect(doc.activeElement).toBe(m.item('Pinned'));
    await m.key('ArrowDown');
    expect(doc.activeElement).toBe(m.item('In the text'));
    await m.key('End');
    expect(doc.activeElement).toBe(m.item('Pinned'));
    await m.key('Home');
    expect(doc.activeElement).toBe(m.item('In the text'));

    await m.escape();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('closes on Tab, leaving the place as it was', async () => {
    const m = await mount('text');
    await m.press();
    await m.key('ArrowDown');
    await m.key('Tab');
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('closes on a press outside it', async () => {
    const m = await mount('text');
    await m.press();
    await m.pressOutside();
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('saves the place you pick and hands focus back to the button', async () => {
    const m = await mount('pinned');
    await m.press();
    await m.key('ArrowDown');
    await m.key('ArrowDown');
    const lifted = m.item('Lifted');
    expect(doc.activeElement).toBe(lifted);
    // Enter and Space press the item, as a click does.
    await act(async () => on(lifted).onClick());
    expect(m.onChange).toHaveBeenCalledTimes(1);
    expect(m.onChange).toHaveBeenCalledWith('lifted');
    expect(m.menu()).toBeNull();
    expect(doc.activeElement).toBe(m.button);
  });

  it('closes without a save when you pick the place your prompt shows now', async () => {
    const m = await mount('pinned');
    await m.press();
    await act(async () => on(m.item('Pinned')).onClick());
    expect(m.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
    expect(doc.activeElement).toBe(m.button);
  });

  it('opens no menu while the profile reads no prompt', async () => {
    const m = await mount('text', { ...reads, capture: false });
    expect(m.button.getAttribute('aria-disabled')).toBe('true');
    await m.press();
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    const waiting = await mount('text', null);
    await waiting.press();
    expect(waiting.menu()).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });

  it('shuts an open menu when the profile stops reading a prompt', async () => {
    const m = await mount('lifted');
    await m.press();
    expect(m.menu()).not.toBeNull();
    await m.update({ ...reads, capture: false, gameSent: false });
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-expanded')).toBe('false');
    expect(m.button.getAttribute('title')).toBe("Tell Vosh your game's prompt first.");
    // It stays shut when the profile reads one again.
    await m.update(reads);
    expect(m.menu()).toBeNull();
    expect(m.button.getAttribute('aria-disabled')).toBeNull();
    expect(m.onChange).not.toHaveBeenCalled();
  });
});
