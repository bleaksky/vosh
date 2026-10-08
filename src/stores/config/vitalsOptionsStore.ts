import { getUiConfig } from '../../ipc/uiConfig';
import {
  DEFAULT_VITALS_OPTIONS,
  subscribeVitalsOptionsChanged,
  vitalsOptionsOf,
  type VitalsColors,
  type VitalsOptions,
} from '../../ipc/uiConfigVitals';
import { createConfigStore } from './configStore';

// The active profile's vitals options for the panel footer, the status
// line and the vitals menu: your style, where your vitals show, and
// every choice under Customize vitals and Hide vitals while your prompt
// is pinned, from Settings, Layout, Vitals.

function sameList<T>(a: readonly T[], b: readonly T[]): boolean {
  return a.length === b.length && a.every((item, i) => item === b[i]);
}

function sameColors(a: VitalsColors, b: VitalsColors): boolean {
  return a.hp === b.hp && a.mana === b.mana && a.move === b.move;
}

/** Keep the current snapshot when nothing in it moved, so the footer
 *  and the status line do not render again. */
function same(a: VitalsOptions, b: VitalsOptions): boolean {
  return (
    a.style === b.style &&
    a.place === b.place &&
    sameList(a.order, b.order) &&
    sameList(a.off, b.off) &&
    a.opponent === b.opponent &&
    sameColors(a.colors, b.colors) &&
    a.values === b.values &&
    a.meter === b.meter &&
    a.warn_thirds === b.warn_thirds &&
    a.hide_when_pinned === b.hide_when_pinned
  );
}

const store = createConfigStore<VitalsOptions>({
  initial: DEFAULT_VITALS_OPTIONS,
  read: () => getUiConfig().then(vitalsOptionsOf),
  follow: subscribeVitalsOptionsChanged,
  same,
});

export const startVitalsOptionsStore = store.start;
export const getVitalsOptions = store.get;
export const useVitalsOptions = store.use;
