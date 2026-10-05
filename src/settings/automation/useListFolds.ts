import { useCallback, useState } from 'react';
import {
  foldsInView,
  isFiltering,
  loadFolded,
  saveFolded,
  withFold,
  type SearchFolds,
} from '../../automation/automationList';

/** The page's storage, or null where reading it throws. */
function pageStorage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

export interface ListFolds {
  /** The groups the list shows folded now, by fold key. */
  folded: ReadonlySet<string>;
  /** Fold or open a group from its heading. While the filter has text
   *  this lasts until the text changes, and the folds you keep stay. */
  setFold: (key: string, fold: boolean) => void;
  /** Open a group for good, like the one that holds the row you picked. */
  open: (key: string) => void;
}

/** The folded groups of one Automation list. The list remembers them
 *  in localStorage, and while `filter` has text every group with a
 *  match shows open. */
export function useListFolds(list: string, filter: string): ListFolds {
  const [kept, setKept] = useState<ReadonlySet<string>>(() => loadFolded(pageStorage(), list));
  const [search, setSearch] = useState<SearchFolds | null>(null);
  // Folds made under one filter end once the text changes, so typing
  // the same text again later opens every group with a match.
  if (search !== null && search.filter !== filter) setSearch(null);
  const folded = foldsInView(kept, search, filter);

  const keep = useCallback(
    (next: ReadonlySet<string>) => {
      setKept(next);
      saveFolded(pageStorage(), list, next);
    },
    [list],
  );

  const setFold = useCallback(
    (key: string, fold: boolean) => {
      if (isFiltering(filter)) {
        const now = foldsInView(kept, search, filter);
        const next = withFold(now, key, fold);
        if (next !== now) setSearch({ filter, folded: next });
        return;
      }
      const next = withFold(kept, key, fold);
      if (next !== kept) keep(next);
    },
    [filter, kept, search, keep],
  );

  const open = useCallback(
    (key: string) => {
      const next = withFold(kept, key, false);
      if (next !== kept) keep(next);
      if (search?.folded.has(key)) {
        setSearch({ filter: search.filter, folded: withFold(search.folded, key, false) });
      }
    },
    [kept, search, keep],
  );

  return { folded, setFold, open };
}
