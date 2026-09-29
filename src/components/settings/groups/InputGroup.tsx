import { LegacyCommandLine, LegacyIsland, LegacyPrompt } from '../legacy/LegacyEditors';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Section } from '../ui';

// Input has no approved board yet. Until it does, the old General
// tab's input and prompt rows render here as they were.
export function InputGroup({ config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const loading = <div className="settings-loading">loading…</div>;
  return (
    <>
      <Section id="command-line" title="Command line">
        <LegacyIsland>
          {config ? <LegacyCommandLine config={config} update={update} /> : loading}
        </LegacyIsland>
      </Section>
      <Section id="prompt" title="Prompt">
        <LegacyIsland>
          {config ? <LegacyPrompt config={config} update={update} /> : loading}
        </LegacyIsland>
      </Section>
    </>
  );
}
