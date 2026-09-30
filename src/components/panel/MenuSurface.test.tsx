import type { ReactElement } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { MenuItem } from './MenuSurface';

// MenuItem uses no hooks, so a test can call it and hand its button the
// events a player sends, with no page to render into.

interface ButtonProps {
  'aria-disabled'?: boolean;
  onKeyDown: (e: { key: string; preventDefault: () => void; stopPropagation: () => void }) => void;
  onClick: () => void;
  onPointerEnter: () => void;
}

/** The row's button and a spy on its submenu. */
function row(disabled: boolean) {
  const onOpen = vi.fn();
  const onSelect = vi.fn();
  const li = MenuItem({
    children: 'Marker',
    disabled,
    onSelect,
    submenu: { open: false, controls: 'pane-menu-marker', onOpen },
  }) as ReactElement<{ children: ReactElement<ButtonProps> }>;
  const button = li.props.children.props;
  const key = (k: string) =>
    button.onKeyDown({ key: k, preventDefault: vi.fn(), stopPropagation: vi.fn() });
  return { button, key, onOpen, onSelect };
}

describe('MenuItem', () => {
  it('opens its submenu from the keyboard, a click, or the pointer', () => {
    for (const k of ['ArrowRight', 'Enter', ' ']) {
      const { key, onOpen } = row(false);
      key(k);
      expect(onOpen, k).toHaveBeenCalledWith(true);
    }
    const { button, onOpen } = row(false);
    button.onClick();
    button.onPointerEnter();
    expect(onOpen.mock.calls).toEqual([[true], [false]]);
  });

  it('keeps a disabled row shut, even when a click left it focused', () => {
    const { button, key, onOpen, onSelect } = row(true);
    expect(button['aria-disabled']).toBe(true);
    for (const k of ['ArrowRight', 'Enter', ' ']) key(k);
    button.onClick();
    button.onPointerEnter();
    expect(onOpen).not.toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });
});
