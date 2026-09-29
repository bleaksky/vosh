import { useEffect, useState } from 'react';
import { ProfilesTab } from '../../ProfilesTab';
import { TrackedAffectsEditor } from '../../TrackedAffectsEditor';
import {
  profilesList,
  setUiConfig,
  subscribeProfilesChanged,
  subscribeProfileSwitched,
  type UiConfig,
} from '../../../lib/session';
import { LegacyIsland } from '../legacy/LegacyEditors';
import type { SettingsPageProps } from '../pageTypes';
import { Section } from '../ui';

// Placeholder for the Characters board (SettingsCharacters.dc.html).
// One Section per board heading holds the old editor for that area:
// the old Profiles tab under the profile's name, and the old tracked
// affects editor under Tracked affects. Both edit the active profile
// only, which the board's per profile editing replaces. Panel layout
// has no old editor, so it only says what the section is for. Replace
// this whole component with the board.

/** The reserved `default` profile reads Default. */
function profileLabel(name: string): string {
  return name === 'default' ? 'Default' : name;
}

export function CharactersGroup({ config, setConfig, onError }: SettingsPageProps) {
  const [active, setActive] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unsubs: (() => void)[] = [];
    const reload = () =>
      profilesList()
        .then((list) => {
          if (!cancelled) setActive(list.active);
        })
        .catch(() => {});
    void reload();
    for (const subscribe of [subscribeProfilesChanged, subscribeProfileSwitched]) {
      void subscribe(() => {
        if (!cancelled) void reload();
      }).then((fn) => {
        if (cancelled) fn();
        else unsubs.push(fn);
      });
    }
    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
    };
  }, []);

  // The old Panels tab saved tracked affects this way: the whole
  // snapshot, at once.
  const update = (patch: Partial<UiConfig>) => {
    setConfig((prev) => {
      if (!prev) return prev;
      const next: UiConfig = { ...prev, ...patch };
      void setUiConfig(next).catch((e) => onError(String(e)));
      return next;
    });
  };

  return (
    <>
      <Section id="profile" title={active ? profileLabel(active) : 'Profiles'}>
        <LegacyIsland>
          <ProfilesTab onError={onError} />
        </LegacyIsland>
      </Section>
      <Section id="tracked" title="Tracked affects">
        <p className="st-note">The Affects pane lists these first and marks any you are missing.</p>
        <LegacyIsland>
          {config ? (
            <TrackedAffectsEditor config={config} update={update} />
          ) : (
            <div className="settings-loading">loading…</div>
          )}
        </LegacyIsland>
      </Section>
      <Section id="layout" title="Panel layout">
        <p className="st-note" data-interim="">
          Vosh saves the panes you arrange for each character.
        </p>
      </Section>
    </>
  );
}
