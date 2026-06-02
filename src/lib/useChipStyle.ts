import { useEffect, useState } from 'react';
import { getUiConfig, subscribeChipStyleChanged, type ChipStyle } from './session';

// Hook reading the live chip-rendering style. Loads the initial value
// from UiConfig once and then tracks vosh://chip-style-changed so a
// picker click in Settings settles across every chip in every window
// without forcing a per-chip re-fetch.
//
// Singleton-style cache so the N chips that might be on screen at
// once share one event subscription instead of opening a separate
// listener per chip.
let cached: ChipStyle | null = null;
const listeners = new Set<(style: ChipStyle) => void>();
let subInflight = false;

function ensureSubscribed() {
  if (subInflight || listeners.size === 0) return;
  subInflight = true;
  void subscribeChipStyleChanged((style) => {
    cached = style;
    for (const cb of listeners) cb(style);
  });
}

export function useChipStyle(): ChipStyle {
  const [style, setStyle] = useState<ChipStyle>(cached ?? 'value_only');
  useEffect(() => {
    if (cached === null) {
      getUiConfig()
        .then((cfg) => {
          cached = cfg.chip_style;
          setStyle(cfg.chip_style);
          for (const cb of listeners) cb(cfg.chip_style);
        })
        .catch(() => {});
    } else {
      setStyle(cached);
    }
    listeners.add(setStyle);
    ensureSubscribed();
    return () => {
      listeners.delete(setStyle);
    };
  }, []);
  return style;
}
