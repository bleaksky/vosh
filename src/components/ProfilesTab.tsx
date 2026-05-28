import { useEffect, useState } from 'react';
import {
  profileCreate,
  profileDelete,
  profileDuplicate,
  profileRename,
  profileSetMetadata,
  profileSwitch,
  profilesList,
  subscribeProfilesChanged,
  subscribeProfileSwitched,
  type ProfileEntry,
  type ProfilesList,
} from '../lib/session';

interface Props {
  onError: (e: string | null) => void;
}

// Settings tab for the named-profile catalog. Lists every profile,
// marks which one is active, lets the user create / rename / delete
// / duplicate / switch, and edit each profile's auto-match block
// (host + port + optional character) for the connect-time auto-pick.
export function ProfilesTab({ onError }: Props) {
  const [data, setData] = useState<ProfilesList | null>(null);
  const [createDraft, setCreateDraft] = useState('');
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');

  const reload = async () => {
    try {
      setData(await profilesList());
    } catch (e) {
      onError(String(e));
    }
  };

  useEffect(() => {
    void reload();
    let cancelled = false;
    let unsub: (() => void) | undefined;
    let unsubSwitched: (() => void) | undefined;
    subscribeProfilesChanged(() => {
      if (!cancelled) void reload();
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    subscribeProfileSwitched(() => {
      if (!cancelled) void reload();
    }).then((fn) => {
      if (cancelled) fn();
      else unsubSwitched = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
      unsubSwitched?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="settings-loading">loading…</div>;
  }

  const handleCreate = async () => {
    const name = createDraft.trim();
    if (!name) return;
    try {
      await profileCreate(name);
      setCreateDraft('');
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`delete profile "${name}"? this also removes its profiles/${name}.toml file.`)) {
      return;
    }
    try {
      await profileDelete(name);
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  const handleSwitch = async (name: string) => {
    try {
      await profileSwitch(name);
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  const handleDuplicate = async (source: string) => {
    const newName = prompt(`duplicate "${source}" as:`, `${source}-copy`);
    if (!newName) return;
    try {
      await profileDuplicate(source, newName.trim());
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  const beginRename = (name: string) => {
    setRenameTarget(name);
    setRenameDraft(name);
  };

  const commitRename = async () => {
    if (!renameTarget) return;
    const newName = renameDraft.trim();
    if (!newName || newName === renameTarget) {
      setRenameTarget(null);
      return;
    }
    try {
      await profileRename(renameTarget, newName);
      setRenameTarget(null);
      onError(null);
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="profiles-tab">
      <div className="profiles-help">
        Each profile carries its own aliases, triggers, macros, quick-keys, and variables. Set host
        + port (and optionally character) below and the matching profile auto-loads when you
        connect.
      </div>

      <div className="profiles-create-row">
        <input
          type="text"
          spellCheck={false}
          value={createDraft}
          placeholder="new profile name (e.g. aabahran-erelei)"
          onChange={(e) => setCreateDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void handleCreate();
          }}
        />
        <button
          type="button"
          className="settings-btn"
          onClick={() => void handleCreate()}
          disabled={!createDraft.trim()}
        >
          [+ new]
        </button>
      </div>

      <div className="profiles-list">
        {data.profiles.map((p) => (
          <ProfileRow
            key={p.name}
            entry={p}
            isActive={p.name === data.active}
            renaming={renameTarget === p.name}
            renameDraft={renameDraft}
            onRenameDraft={setRenameDraft}
            onBeginRename={() => beginRename(p.name)}
            onCommitRename={() => void commitRename()}
            onCancelRename={() => setRenameTarget(null)}
            onSwitch={() => void handleSwitch(p.name)}
            onDelete={() => void handleDelete(p.name)}
            onDuplicate={() => void handleDuplicate(p.name)}
            onSaveAutoMatch={async (am, description) => {
              try {
                await profileSetMetadata(p.name, description, am);
                onError(null);
              } catch (e) {
                onError(String(e));
              }
            }}
          />
        ))}
      </div>
    </div>
  );
}

interface RowProps {
  entry: ProfileEntry;
  isActive: boolean;
  renaming: boolean;
  renameDraft: string;
  onRenameDraft: (v: string) => void;
  onBeginRename: () => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onSwitch: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onSaveAutoMatch: (
    am: { host: string | null; port: number | null; character: string | null } | null,
    description: string | null,
  ) => void;
}

function ProfileRow({
  entry,
  isActive,
  renaming,
  renameDraft,
  onRenameDraft,
  onBeginRename,
  onCommitRename,
  onCancelRename,
  onSwitch,
  onDelete,
  onDuplicate,
  onSaveAutoMatch,
}: RowProps) {
  const [open, setOpen] = useState(false);
  const [hostDraft, setHostDraft] = useState(entry.auto_match?.host ?? '');
  const [portDraft, setPortDraft] = useState(
    entry.auto_match?.port ? String(entry.auto_match.port) : '',
  );
  const [charDraft, setCharDraft] = useState(entry.auto_match?.character ?? '');
  const [descDraft, setDescDraft] = useState(entry.description ?? '');

  useEffect(() => {
    setHostDraft(entry.auto_match?.host ?? '');
    setPortDraft(entry.auto_match?.port ? String(entry.auto_match.port) : '');
    setCharDraft(entry.auto_match?.character ?? '');
    setDescDraft(entry.description ?? '');
  }, [entry]);

  const handleSaveMatch = () => {
    const host = hostDraft.trim();
    const port = portDraft.trim() ? Number(portDraft.trim()) : null;
    const character = charDraft.trim();
    const description = descDraft.trim();
    const hasAny = host || port !== null || character;
    onSaveAutoMatch(
      hasAny
        ? {
            host: host || null,
            port: port && Number.isFinite(port) ? port : null,
            character: character || null,
          }
        : null,
      description || null,
    );
  };

  return (
    <div className={`profile-row${isActive ? ' is-active' : ''}`}>
      <div className="profile-row-head">
        <span className="profile-row-marker" aria-hidden="true">
          {isActive ? '●' : '○'}
        </span>
        {renaming ? (
          <input
            type="text"
            className="profile-row-rename"
            autoFocus
            spellCheck={false}
            value={renameDraft}
            onChange={(e) => onRenameDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') onCommitRename();
              if (e.key === 'Escape') onCancelRename();
            }}
            onBlur={onCommitRename}
          />
        ) : (
          <span className="profile-row-name">{entry.name}</span>
        )}
        {entry.description && !renaming && (
          <span className="profile-row-desc">{entry.description}</span>
        )}
        <div className="profile-row-actions">
          {!isActive && (
            <button type="button" className="settings-btn" onClick={onSwitch}>
              [switch]
            </button>
          )}
          <button type="button" className="settings-btn" onClick={() => setOpen((v) => !v)}>
            {open ? '[hide]' : '[auto-match]'}
          </button>
          <button type="button" className="settings-btn settings-btn-mute" onClick={onBeginRename}>
            [rename]
          </button>
          <button type="button" className="settings-btn settings-btn-mute" onClick={onDuplicate}>
            [duplicate]
          </button>
          {!isActive && (
            <button type="button" className="settings-btn settings-btn-danger" onClick={onDelete}>
              [delete]
            </button>
          )}
        </div>
      </div>

      {open && (
        <div className="profile-row-detail">
          <label className="profile-row-field">
            <span>description</span>
            <input
              type="text"
              spellCheck={false}
              value={descDraft}
              placeholder="optional. e.g. Aabahran on Erelei"
              onChange={(e) => setDescDraft(e.target.value)}
            />
          </label>
          <div className="profile-row-match-grid">
            <label className="profile-row-field">
              <span>host</span>
              <input
                type="text"
                spellCheck={false}
                value={hostDraft}
                placeholder="play.example.com"
                onChange={(e) => setHostDraft(e.target.value)}
              />
            </label>
            <label className="profile-row-field">
              <span>port</span>
              <input
                type="text"
                spellCheck={false}
                value={portDraft}
                placeholder="optional. e.g. 1848"
                onChange={(e) => setPortDraft(e.target.value)}
              />
            </label>
            <label className="profile-row-field">
              <span>character</span>
              <input
                type="text"
                spellCheck={false}
                value={charDraft}
                placeholder="optional. used when one MUD has many"
                onChange={(e) => setCharDraft(e.target.value)}
              />
            </label>
          </div>
          <div className="profile-row-detail-actions">
            <button type="button" className="settings-btn" onClick={handleSaveMatch}>
              [save]
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
