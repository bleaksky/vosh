// The colored copy of what you type, drawn under the textarea. The
// textarea keeps the caret, the selection and spell check, and turns its
// own text transparent while this layer draws.

import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import type { KnownWords } from '../ipc/input';
import type { TypeColors } from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { getCurrentThemeId, subscribeThemeChanges } from '../theme/theme';
import { findTheme } from '../theme/themes';
import { typeSpans } from './typeColors';

interface Props {
  /** The command line textarea the layer sits under. */
  field: HTMLTextAreaElement | null;
  value: string;
  words: KnownWords;
  colors: TypeColors;
}

/** The kind colors, yours or the theme's terminal cyan, magenta and
 *  yellow. An unknown # command falls back to --danger-text in CSS. */
function kindColors(colors: TypeColors, themeId: string): CSSProperties {
  const xterm = findTheme(themeId).xterm;
  const picked: [string, string | null | undefined][] = [
    ['--type-alias', colors.alias ?? xterm.cyan],
    ['--type-hash', colors.hash ?? xterm.magenta],
    ['--type-chat', colors.chat ?? xterm.yellow],
    ['--type-unknown', colors.unknown],
  ];
  return Object.fromEntries(picked.filter(([, color]) => color)) as CSSProperties;
}

export function TypeColorLayer({ field, value, words, colors }: Props) {
  const layerRef = useRef<HTMLDivElement | null>(null);
  const [themeId, setThemeId] = useState(getCurrentThemeId);
  useTauriEvent(subscribeThemeChanges, setThemeId);
  const [box, setBox] = useState<{ left: number; top: number; width: number; height: number }>();

  // Sit exactly over the textarea's text box, and follow it as the row
  // reflows.
  useLayoutEffect(() => {
    if (!field) return;
    const place = () => {
      const next = {
        left: field.offsetLeft,
        top: field.offsetTop,
        width: field.clientWidth,
        height: field.clientHeight,
      };
      setBox((prev) =>
        prev &&
        prev.left === next.left &&
        prev.top === next.top &&
        prev.width === next.width &&
        prev.height === next.height
          ? prev
          : next,
      );
    };
    // A tall compose scrolls inside the textarea, and the layer scrolls
    // with it.
    const scroll = () => {
      if (layerRef.current) layerRef.current.scrollTop = field.scrollTop;
    };
    place();
    scroll();
    field.addEventListener('scroll', scroll);
    const watch = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(place);
    watch?.observe(field);
    return () => {
      field.removeEventListener('scroll', scroll);
      watch?.disconnect();
    };
  }, [field, value]);

  return (
    <div
      ref={layerRef}
      className="input-type-layer"
      aria-hidden="true"
      style={{ ...box, ...kindColors(colors, themeId) }}
    >
      {typeSpans(value, words).map((span, i) =>
        span.kind ? (
          <span key={i} className={`input-type-${span.kind}`}>
            {span.text}
          </span>
        ) : (
          span.text
        ),
      )}
      {value.endsWith('\n') ? '\u200b' : null}
    </div>
  );
}
