import { getUiConfig, subscribeVitalsDensityChanged, type VitalsDensity } from '../../ipc/uiConfig';
import { createConfigStore } from './configStore';

// The active profile's vitals density for the panel footer, which you
// pick under Density in Settings, Layout, Vitals.

const store = createConfigStore<VitalsDensity>({
  initial: 'rows',
  read: () => getUiConfig().then((cfg) => cfg.vitals_density),
  follow: subscribeVitalsDensityChanged,
});

export const startVitalsDensityStore = store.start;
export const getVitalsDensity = store.get;
export const useVitalsDensity = store.use;
