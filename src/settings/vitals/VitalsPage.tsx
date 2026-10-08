import { CustomizeVitalsSection } from '../layout/VitalsCustomize';
import { VitalsSection } from '../layout/LayoutPage';
import type { SettingsPageProps } from '../pageTypes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';

// Vitals (Settings layout, answered October 8): Style, with the gallery,
// Show your vitals in and Hide vitals while your prompt is pinned, then
// Customize vitals, both moved whole from Layout with their anchors
// (Q4). The section keeps its id, vitals, and takes the title Style so
// the tab does not say Vitals twice. The vitals pane menu's Customize
// vitals… lands here.

export function VitalsPage({ config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  if (!config) return null;
  return (
    <>
      <VitalsSection config={config} update={update} />
      <CustomizeVitalsSection config={config} update={update} />
    </>
  );
}
