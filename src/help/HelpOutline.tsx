import { useEffect, useRef, useState, type RefObject } from 'react';
import { helpItemId, type OutlineEntry } from './helpNav';
import { scrollWithin } from '../lib/scrollWithin';

// On this page: beside a long reference list, a row per item in mono
// 11.5 on the 24 px pitch. The item you are reading carries the selected
// row fill, and a click scrolls to the item and flashes it the way a
// Settings search hit flashes its row.

/** How far under the band an item sits once the outline scrolls to
 *  it, and the line that decides which item you are reading. */
const READ_LINE = 48;

/** How long the flash runs, the Settings `st-flash`. */
const FLASH_MS = 1200;

interface Props {
  entries: OutlineEntry[];
  /** The column the article scrolls in. */
  scrollRef: RefObject<HTMLElement | null>;
}

/** The entry you are reading: the last one whose item has reached the
 *  read line. At the end of the page the last items can never reach
 *  it, so the one you picked there stays, or else the last one. */
function readingIndex(entries: OutlineEntry[], root: HTMLElement, picked: number | null): number {
  const box = root.getBoundingClientRect();
  const line = box.top + READ_LINE + 1;
  let reading = 0;
  entries.forEach((entry, i) => {
    const el = document.getElementById(helpItemId(entry.block, entry.item));
    if (el && el.getBoundingClientRect().top <= line) reading = i;
  });
  const atEnd = root.scrollTop + root.clientHeight >= root.scrollHeight - 1;
  if (!atEnd) return reading;
  if (picked !== null && picked >= reading) return picked;
  return entries.length - 1;
}

export function HelpOutline({ entries, scrollRef }: Props) {
  const [reading, setReading] = useState(0);
  // The item you picked last, until you scroll on your own.
  const pickedRef = useRef<number | null>(null);

  useEffect(() => {
    const root = scrollRef.current;
    if (!root) return;
    let frame = 0;
    const read = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() =>
        setReading(readingIndex(entries, root, pickedRef.current)),
      );
    };
    const own = () => {
      pickedRef.current = null;
    };
    root.addEventListener('wheel', own, { passive: true });
    root.addEventListener('keydown', own);
    read();
    root.addEventListener('scroll', read, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      root.removeEventListener('scroll', read);
      root.removeEventListener('wheel', own);
      root.removeEventListener('keydown', own);
    };
  }, [entries, scrollRef]);

  const go = (entry: OutlineEntry, index: number) => {
    const el = document.getElementById(helpItemId(entry.block, entry.item));
    if (!el) return;
    pickedRef.current = index;
    setReading(index);
    scrollWithin(el, { block: 'start' });
    el.classList.remove('st-flash');
    // Restart the flash when you pick the same item twice.
    void el.offsetWidth;
    el.classList.add('st-flash');
    window.setTimeout(() => el.classList.remove('st-flash'), FLASH_MS);
  };

  return (
    <nav className="hp-outline" aria-labelledby="hp-outline-heading">
      <h2 id="hp-outline-heading" className="hp-outline-heading">
        On this page
      </h2>
      <ul>
        {entries.map((entry, i) => {
          const id = helpItemId(entry.block, entry.item);
          return (
            <li key={id}>
              <a
                href={`#${id}`}
                className="hp-outline-item"
                aria-current={i === reading ? 'location' : undefined}
                onClick={(e) => {
                  e.preventDefault();
                  go(entry, i);
                }}
              >
                {entry.label}
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
