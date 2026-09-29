import { LogsTab } from '../../LogsTab';
import { ProfileScopeEditor } from '../../ProfilesTab';
import { isMacPlatform } from '../../../lib/palette';
import { LegacyIsland, LegacyRendering, LegacyUpdates } from '../legacy/LegacyEditors';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Section } from '../ui';

// General has no approved board yet. Until it does, the rows the old
// General tab kept after Input took the command line rows render here
// as they were: updates, the GPU rendering toggle (Windows and Linux
// only, since it drives the xterm renderer macOS does not show), the
// profile scope rows from the old Profiles tab, and session logs.
export function GeneralGroup({ config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const mac = isMacPlatform();
  return (
    <>
      <Section
        id="updates"
        title="Updates"
        actions={<span className="st-meta">Vosh {__APP_VERSION__}</span>}
      >
        <LegacyIsland>
          {config ? (
            <LegacyUpdates config={config} update={update} />
          ) : (
            <div className="settings-loading">loading…</div>
          )}
        </LegacyIsland>
      </Section>
      {!mac && (
        <Section id="rendering" title="Rendering">
          <LegacyIsland>
            <LegacyRendering />
          </LegacyIsland>
        </Section>
      )}
      <Section id="scope" title="Profile scope">
        <LegacyIsland>
          <ProfileScopeEditor onError={onError} />
        </LegacyIsland>
      </Section>
      <Section id="logs" title="Session logs">
        <LegacyIsland>
          <LogsTab onError={onError} />
        </LegacyIsland>
      </Section>
    </>
  );
}
