import {
  DEFAULT_VITALS_OPTIONS,
  getUiConfig,
  subscribeVitalsOptionsChanged,
  vitalsOptionsOf,
  type VitalsOptions,
} from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The active profile's vitals options for the panel footer and the
// status line: Values, Meter, Warn before you run low, and Hide vitals
// while your prompt is pinned from Settings, Layout, Vitals. Density
// keeps its own store.

/** Keep the current snapshot when nothing in it moved, so the footer
 *  and the status line do not render again. */
function same(a: VitalsOptions, b: VitalsOptions): boolean {
  return (
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
