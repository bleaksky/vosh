import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  findProfileName,
  hasWorld,
  loginCharacter,
  loginLabel,
  loginSentence,
  profileDisplayName,
  worldKey,
  worldOptions,
  worldSources,
} from '../../lib/characterProfiles';
import { resetPaneLayout, subscribePaneLayout } from '../../panel/paneLayout';
import {
  subscribeTrackedAffectsChanged,
  trackedAffectsSet,
  type TrackedAffect,
} from '../../ipc/affects';
import {
  profileDetailGet,
  profileSetLogin,
  profileSetWorld,
  sessionIdentityGet,
  subscribeProfileChanged,
  subscribeSessionIdentity,
  type ProfileDetail,
  type SessionIdentity,
} from '../../ipc/characters';
import {
  profilesList,
  subscribeProfilesChanged,
  subscribeProfileSwitched,
  type ProfilesList,
} from '../../ipc/profiles';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { loadTarget } from '../../stores/session/useConnection';
import type { SettingsPageProps } from '../pageTypes';
import { Row, Section, Select, Toggle } from '../../ui';
import { ImportSheet } from './ImportSheet';
import { PanelLayout } from './PanelLayout';
import { ProfileAdvanced } from './ProfileAdvanced';
import { ProfileList } from './ProfileList';
import type { ImportFile } from './profileImport';
import { TrackedAffects } from './TrackedAffects';

// Settings > Characters. The profile list on the left, and on the right
// the selected profile: its login toggle and world, its tracked
// affects, and its panel layout, then a quiet Advanced row with a label
// and an order for each tracked affect. The page leaves out the
// description, host, port, and extra character names, so they stay in
// the data and off the page. Selecting a profile edits it in place,
// active or not, and never switches the live session. A deep link names
// the profile as its section (`characters:Ilsabet#tracked`), and no
// section means the profile in use.
//
// Every edit goes through the per profile Characters commands, never
// the whole UI config, so editing an inactive profile cannot reach the
// main window. The page follows edits from anywhere through the
// profile events and the session identity event.
//
// A profile export you pick with Import… takes the detail column as the
// import sheet, and no row reads selected while it shows. Cancel, a
// press on a row or a link puts the profile back, and an import selects
// the profile the file went to.

export function CharactersPage({ target, navSeq, setConfig, onError }: SettingsPageProps) {
  const [list, setList] = useState<ProfilesList | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<ProfileDetail | null>(null);
  const [identity, setIdentity] = useState<SessionIdentity | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [resetting, setResetting] = useState(false);
  // The export the import sheet shows. Each pick counts, so a second
  // pick of a file with the same name starts a fresh sheet.
  const [importing, setImporting] = useState<{ file: ImportFile; pick: number } | null>(null);

  // The profile a navigation asked for, applied once the list shows
  // it. No section means the profile in use.
  const pending = useRef<{ section: string | undefined } | null>({ section: target.section });
  const listRef = useRef<ProfilesList | null>(null);
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selected;
  // Answers that arrive after a newer request are dropped.
  const detailSeq = useRef(0);
  const trackedSeq = useRef(0);
  const trackedRef = useRef<TrackedAffect[]>([]);
  trackedRef.current = detail?.tracked_affects ?? [];

  const choose = useCallback((l: ProfilesList) => {
    const names = l.profiles.map((p) => p.name);
    const nav = pending.current;
    if (nav) {
      pending.current = null;
      setSelected((nav.section && findProfileName(names, nav.section)) || l.active);
      return;
    }
    setSelected((prev) => (prev && names.includes(prev) ? prev : l.active));
  }, []);

  const reloadList = useCallback(() => {
    profilesList()
      .then((l) => {
        listRef.current = l;
        setList(l);
        choose(l);
      })
      .catch((e: unknown) => onError(String(e)));
  }, [choose, onError]);

  const reloadDetail = useCallback(() => {
    const name = selectedRef.current;
    if (!name) return;
    const seq = ++detailSeq.current;
    profileDetailGet(name)
      .then((d) => {
        if (seq === detailSeq.current && d.name === selectedRef.current) setDetail(d);
      })
      .catch((e: unknown) => {
        // A profile renamed or deleted since the read is not an error.
        if (seq === detailSeq.current && name === selectedRef.current) onError(String(e));
      });
  }, [onError]);

  const reloadAll = useCallback(() => {
    reloadList();
    reloadDetail();
  }, [reloadList, reloadDetail]);

  // Each navigation, even to the same link, picks its profile again,
  // and closes the import sheet.
  useEffect(() => {
    pending.current = { section: target.section };
    setImporting(null);
    if (listRef.current) choose(listRef.current);
  }, [navSeq, target.section, choose]);

  useEffect(() => {
    reloadDetail();
  }, [selected, reloadDetail]);

  useEffect(() => {
    reloadList();
    sessionIdentityGet()
      .then(setIdentity)
      .catch(() => {});
  }, [reloadList]);

  useTauriEvent(subscribeProfilesChanged, () => reloadAll());
  useTauriEvent(subscribeProfileSwitched, () => reloadAll());
  useTauriEvent(subscribeProfileChanged, (name) => {
    if (name === selectedRef.current) reloadDetail();
  });
  useTauriEvent(subscribeSessionIdentity, setIdentity);
  // The live profile's list and panes can change in the main
  // window. Keep the window's config copy on the live list, so a
  // full save from another page never writes an old one back.
  useTauriEvent(subscribeTrackedAffectsChanged, (tracked) => {
    setConfig((prev) => (prev ? { ...prev, tracked_affects: tracked } : prev));
    if (selectedRef.current === listRef.current?.active) reloadDetail();
  });
  useTauriEvent(subscribePaneLayout, () => {
    if (selectedRef.current === listRef.current?.active) reloadDetail();
  });

  const select = (name: string) => {
    if (name === selected && !importing) return;
    setImporting(null);
    setStatus(null);
    setSelected(name);
  };

  // Show `name` once the list holds it, after a create, a copy, a
  // rename or an import.
  const show = (name: string) => {
    setImporting(null);
    pending.current = { section: name };
    setSelected(name);
  };

  // Tracked affects save to the selected profile after every edit. The
  // chips change at once. An answer from an older save is dropped, and
  // a failed save reads the profile back.
  const editTracked = (edit: (list: TrackedAffect[]) => TrackedAffect[]) => {
    const current = detail;
    if (!current) return;
    const next = edit(trackedRef.current);
    if (next === trackedRef.current) return;
    trackedRef.current = next;
    setDetail((d) => (d && d.name === current.name ? { ...d, tracked_affects: next } : d));
    const seq = ++trackedSeq.current;
    trackedAffectsSet(next, current.name)
      .then((saved) => {
        onError(null);
        if (current.active) {
          setConfig((prev) => (prev ? { ...prev, tracked_affects: saved } : prev));
        }
        if (seq !== trackedSeq.current) return;
        setDetail((d) => (d && d.name === current.name ? { ...d, tracked_affects: saved } : d));
      })
      .catch((e: unknown) => {
        onError(String(e));
        reloadDetail();
      });
  };

  const character = detail ? loginCharacter(detail.auto_match, identity) : null;
  const worldSet = detail ? hasWorld(detail.auto_match) : false;

  const setLogin = (on: boolean) => {
    if (!detail || !character) return;
    const name = detail.name;
    setDetail((d) => (d && d.name === name ? { ...d, login_on: on } : d));
    profileSetLogin(name, character, on)
      .then((claim) => {
        onError(null);
        setStatus(on ? loginSentence(character, claim, name) : null);
      })
      .catch((e: unknown) => onError(String(e)))
      .finally(reloadAll);
  };

  // Every world a profile names, the connection you saved, the one you
  // are on, and this profile's own, so the select always holds it.
  const ownHost = detail?.auto_match?.host;
  const ownPort = detail?.auto_match?.port;
  const options = useMemo(
    () =>
      worldOptions([
        ...worldSources(list?.profiles ?? [], loadTarget(), identity),
        { host: ownHost, port: ownPort },
      ]),
    [list, identity, ownHost, ownPort],
  );

  const setWorld = (value: string) => {
    if (!detail) return;
    const option = options.find((o) => o.value === value);
    if (!option) return;
    profileSetWorld(detail.name, option.host, option.port)
      .then(() => onError(null))
      .catch((e: unknown) => onError(String(e)))
      .finally(reloadAll);
  };

  const resetPanes = () => {
    if (!detail) return;
    const name = detail.name;
    setResetting(true);
    resetPaneLayout(name)
      .then((panes) => {
        onError(null);
        setDetail((d) => (d && d.name === name ? { ...d, panes } : d));
      })
      .catch((e: unknown) => onError(String(e)))
      .finally(() => setResetting(false));
  };

  const loginNote = !detail
    ? undefined
    : !character
      ? 'Log in once so Vosh learns your character name.'
      : !worldSet
        ? 'Choose a world first.'
        : undefined;

  return (
    <div className="st-chars">
      {list ? (
        <ProfileList
          list={list}
          selected={importing ? null : selected}
          identity={identity}
          status={status}
          onSelect={select}
          onShow={show}
          onStatus={setStatus}
          onError={onError}
          onChanged={reloadAll}
          onImport={(file) => {
            setStatus(null);
            setImporting((prev) => ({ file, pick: (prev?.pick ?? 0) + 1 }));
          }}
        />
      ) : (
        <div className="st-chars-list" />
      )}
      <div className="st-chars-detail">
        {importing && list && (
          <ImportSheet
            key={importing.pick}
            file={importing.file}
            profiles={list.profiles}
            fallback={selected ?? list.active}
            onImported={(result, sentence) => {
              show(result.name);
              setStatus(sentence);
              reloadAll();
            }}
            onCancel={() => setImporting(null)}
            onError={onError}
          />
        )}
        {!importing && detail && (
          <>
            <Section
              title={detail.display_name || profileDisplayName(detail.name)}
              help={{ topic: 'characters-and-data.profiles', subject: 'characters' }}
            >
              <Row label={loginLabel(character)} description={loginNote} anchor="login">
                <Toggle
                  checked={detail.login_on}
                  disabled={!character || !worldSet}
                  onChange={setLogin}
                />
              </Row>
              <Row label="World" anchor="world">
                <Select
                  width={240}
                  value={worldKey(detail.auto_match?.host, detail.auto_match?.port)}
                  options={options}
                  onChange={setWorld}
                />
              </Row>
            </Section>
            <TrackedAffects
              key={`tracked ${detail.name}`}
              tracked={detail.tracked_affects}
              onEdit={editTracked}
            />
            <PanelLayout
              owner={detail.display_name || profileDisplayName(detail.name)}
              panes={detail.panes}
              onReset={resetPanes}
              resetting={resetting}
            />
            {detail.tracked_affects.length > 0 && (
              <ProfileAdvanced
                key={`advanced ${detail.name}`}
                tracked={detail.tracked_affects}
                onTracked={editTracked}
              />
            )}
          </>
        )}
      </div>
    </div>
  );
}
