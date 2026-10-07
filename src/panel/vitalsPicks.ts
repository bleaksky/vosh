import {
  setUiFields,
  VITALS_STYLES,
  VITALS_VALUES,
  type UiFields,
  type VitalsOptions,
  type VitalsStyle,
  type VitalsValues,
} from '../ipc/uiConfig';
import { broadcastVitalsOptions } from '../ipc/uiConfigBroadcast';
import { getVitalsOptions } from '../stores/config/vitalsOptionsStore';
import type { MenuChoice } from './affects/affectsDisplay';
import { VITALS_STYLE_LABELS, VITALS_VALUES_LABELS, vitalsStylePick } from './vitalsView';

// The choices of the vitals menu (VitalsMenu.tsx) and what a pick does.

export function vitalsStyleChoices(options: VitalsOptions): MenuChoice<VitalsStyle>[] {
  return VITALS_STYLES.map((value) => ({
    value,
    label: VITALS_STYLE_LABELS[value],
    checked: options.style === value,
  }));
}

export function vitalsValuesChoices(options: VitalsOptions): MenuChoice<VitalsValues>[] {
  return VITALS_VALUES.map((value) => ({
    value,
    label: VITALS_VALUES_LABELS[value],
    checked: options.values === value,
  }));
}

/** Save `fields` alone, then send every window this window's options
 *  with `change` made. */
async function pickVitals(fields: UiFields, change: Partial<VitalsOptions>): Promise<void> {
  await setUiFields(fields);
  await broadcastVitalsOptions({ ...getVitalsOptions(), ...change });
}

export function pickVitalsStyle(style: VitalsStyle): Promise<void> {
  return pickVitals(vitalsStylePick(style), { style });
}

export function pickVitalsValues(values: VitalsValues): Promise<void> {
  return pickVitals({ vitals_values: values }, { values });
}
