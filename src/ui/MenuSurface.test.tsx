import type { ReactElement } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { HOLD_MS, notePointer, resetMenuAim } from './menuAim';
import { MenuItem } from './MenuSurface';

// MenuItem uses no hooks, so a test can call it and hand its button the
// events a player sends, with no page to render into.

interface ButtonProps {
  'aria-disabled'?: boolean;
  onKeyDown: (e: { key: string; preventDefault: () => void; stopPropagation: () => void }) => void;
  onClick: () => void;
  onPointerEnter: (e: { currentTarget: FakeRow }) => void;
  onPointerMove: (e: { currentTarget: FakeRow }) => void;
  onPointerLeave: (e: { currentTarget: FakeRow }) => void;
}

/** The button a pointer event lands on. */
interface FakeRow {
  focus: () => void;
  getAttribute: (name: string) => string | null;
}

// The page the row reads: what has focus, and the submenus open beside
// the menu, none unless a test opens one.
const page = {
  activeElement: null as unknown,
  submenus: [] as {
    id: string;
    box: { left: number; right: number; top: number; bottom: number };
  }[],
};

beforeEach(() => {
  resetMenuAim();
  page.activeElement = null;
  page.submenus = [];
  vi.stubGlobal('document', {
    get activeElement() {
      return page.activeElement;
    },
    querySelectorAll: () =>
      page.submenus.map((m) => ({
        id: m.id,
        contains: () => false,
        getBoundingClientRect: () => m.box,
      })),
  });
});
afterEach(() => vi.unstubAllGlobals());

function fakeRow(): FakeRow {
  const el: FakeRow = {
    focus: vi.fn(() => {
      page.activeElement = el;
    }),
    getAttribute: () => null,
  };
  return el;
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
    button.onPointerEnter({ currentTarget: fakeRow() });
    expect(onOpen.mock.calls).toEqual([[true], [false]]);
  });

  it('keeps a disabled row shut, even when a click left it focused', () => {
    const { button, key, onOpen, onSelect } = row(true);
    expect(button['aria-disabled']).toBe(true);
    for (const k of ['ArrowRight', 'Enter', ' ']) key(k);
    button.onClick();
    button.onPointerEnter({ currentTarget: fakeRow() });
    expect(onOpen).not.toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('takes the highlight when the pointer reaches it', () => {
    const { button } = row(false);
    const el = fakeRow();
    button.onPointerMove({ currentTarget: el });
    expect(el.focus).toHaveBeenCalledOnce();
  });
});

describe('MenuItem on the way into a submenu', () => {
  // A row under the one that opened a submenu, with the submenu to the
  // right of the menu, the way the writing card's New and Sent sit.
  const SUB = { id: 'wr-sub-new', box: { left: 370, right: 530, top: 314, bottom: 603 } };

  function sibling() {
    const onHover = vi.fn();
    const onOpen = vi.fn();
    const plain = MenuItem({ children: 'Close pane', onHover }) as ReactElement<{
      children: ReactElement<ButtonProps>;
    }>;
    const opener = MenuItem({
      children: 'Sent',
      submenu: { open: false, controls: 'wr-sub-sent', onOpen },
    }) as ReactElement<{ children: ReactElement<ButtonProps> }>;
    return {
      plain: plain.props.children.props,
      opener: opener.props.children.props,
      onHover,
      onOpen,
    };
  }

  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('leaves the submenu open while the pointer crosses a row toward it', () => {
    page.submenus = [SUB];
    const { plain, opener, onHover, onOpen } = sibling();
    const el = fakeRow();
    notePointer(194, 335);
    notePointer(210, 352);
    plain.onPointerEnter({ currentTarget: el });
    plain.onPointerMove({ currentTarget: el });
    plain.onPointerLeave({ currentTarget: el });
    const next = fakeRow();
    notePointer(226, 368);
    opener.onPointerEnter({ currentTarget: next });
    opener.onPointerLeave({ currentTarget: next });
    vi.advanceTimersByTime(HOLD_MS);
    expect(onHover).not.toHaveBeenCalled();
    expect(onOpen).not.toHaveBeenCalled();
    expect(el.focus).not.toHaveBeenCalled();
    expect(next.focus).not.toHaveBeenCalled();
  });

  it('takes over once the pointer rests on the row', () => {
    page.submenus = [SUB];
    const { opener, onOpen } = sibling();
    const el = fakeRow();
    notePointer(194, 335);
    notePointer(210, 352);
    opener.onPointerEnter({ currentTarget: el });
    expect(onOpen).not.toHaveBeenCalled();
    vi.advanceTimersByTime(HOLD_MS);
    expect(onOpen).toHaveBeenCalledWith(false);
    expect(el.focus).toHaveBeenCalledOnce();
  });

  it('takes over at once when the pointer heads away from the submenu', () => {
    page.submenus = [SUB];
    const { plain, onHover } = sibling();
    notePointer(194, 335);
    notePointer(194, 365);
    plain.onPointerEnter({ currentTarget: fakeRow() });
    expect(onHover).toHaveBeenCalledOnce();
  });
});
