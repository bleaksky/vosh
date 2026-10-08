import { Fragment, isValidElement, type ReactElement, type ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { MenuItem } from '../ui/MenuSurface';
import { draftItem, kindItems, newKindRows } from './kindsMenuRows';

interface ItemProps {
  children: ReactNode;
  onSelect?: () => void;
  onHover?: () => void;
}

/** The menu rows in `node`, in order. */
function items(node: ReactNode): ReactElement<ItemProps>[] {
  const out: ReactElement<ItemProps>[] = [];
  const walk = (n: ReactNode) => {
    if (Array.isArray(n)) n.forEach(walk);
    else if (!isValidElement(n)) return;
    else if (n.type === MenuItem) out.push(n as ReactElement<ItemProps>);
    else if (n.type === Fragment) walk((n.props as { children?: ReactNode }).children);
  };
  walk(node);
  return out;
}

const KINDS = {
  boards: ['note', 'journal', 'application', 'idea', 'bug', 'typo'] as const,
  aboutYou: ['description', 'history'] as const,
};
const kinds = { boards: [...KINDS.boards], aboutYou: [...KINDS.aboutYou] };
const draft = { id: 'd1', kind: 'note' as const, title: 'Market day', meta: '1 line' };

describe('the title menu of the writing card', () => {
  it('keeps the New submenu open as the pointer reaches each of its rows', () => {
    const select = vi.fn();
    const rows = items(newKindRows(kinds, select, null));
    expect(rows).toHaveLength(8);
    for (const row of rows) expect(row.props.onHover).toBeUndefined();
    rows.at(-1)?.props.onSelect?.();
    expect(select).toHaveBeenCalledWith('history');
  });

  it('keeps the Sent submenu open as the pointer reaches a post', () => {
    const row = items(draftItem(draft, vi.fn(), null))[0];
    expect(row?.props.onHover).toBeUndefined();
  });

  it('closes a sibling submenu from a row of the menu itself', () => {
    const closeSub = vi.fn();
    const rows = [
      ...items(newKindRows(kinds, vi.fn(), closeSub)),
      ...items(draftItem(draft, vi.fn(), closeSub)),
      ...items(kindItems(['note'], vi.fn(), closeSub)),
    ];
    expect(rows).toHaveLength(10);
    for (const row of rows) row.props.onHover?.();
    expect(closeSub).toHaveBeenCalledTimes(10);
  });
});
