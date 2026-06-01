import { useEffect, useState } from 'react';
import {
  profileCreate,
  profileDelete,
  profileDuplicate,
  profileGetScope,
  profileRename,
  profileSetMetadata,
  profileSetScope,
  profileSwitch,
  profilesList,
  subscribeProfilesChanged,
  subscribeProfileSwitched,
  type ProfileEntry,
  type ProfileScope,
  type ProfilesList,
  type ScopeConfig,
} from '../lib/session';
import { MigrationWizard } from './MigrationWizard';

interface Props {
  onError: (e: string | null) => void;
}

// Settings tab for the named-profile catalog. Lists every profile,
// marks which one is active, lets the user create / rename / delete
// / duplicate / switch, and edit each profile's auto-match block
// (host + port + optional character) for the connect-time auto-pick.
export function ProfilesTab({ onError }: Props) {
  const [data, setData] = useState<ProfilesList | null>(null);
  const [scope, setScope] = useState<ScopeConfig | null>(null);
  const [createDraft, setCreateDraft] = useState('');
  const [renameTarget, setRenameTarget] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  // Duplicate uses the same inline-input pattern as rename because
  // window.prompt() is disabled in Tauri webviews on both macOS
  // (WKWebView) and Windows (WebView2) — calling it returns null
  // without showing any UI, so the operation silently aborted.
  const [duplicateTarget, setDuplicateTarget] = useState<string | null>(null);
  const [duplicateDraft, setDuplicateDraft] = useState('');
  const [showMigrationWizard, setShowMigrationWizard] = useState(false);

  const reload = async () => {
    try {
      const [list, scopeCfg] = await Promise.all([profilesList(), profileGetScope()]);
      setData(list);
      setScope(scopeCfg);
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

  const beginDuplicate = (name: string) => {
    setDuplicateTarget(name);
    setDuplicateDraft(`${name}-copy`);
  };

  const cancelDuplicate = () => {
    setDuplicateTarget(null);
    setDuplicateDraft('');
  };

  const commitDuplicate = async () => {
    if (!duplicateTarget) return;
    const newName = duplicateDraft.trim();
    if (!newName) {
      cancelDuplicate();
      return;
    }
    try {
      await profileDuplicate(duplicateTarget, newName);
      onError(null);
      cancelDuplicate();
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

  const handleScopeChange = async (field: keyof ScopeConfig, next: ProfileScope) => {
    if (!scope) return;
    const updated = { ...scope, [field]: next };
    setScope(updated);
    try {
      await profileSetScope(updated);
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

      {scope && (
        <div className="scope-section">
          <div className="scope-section-title">scope</div>
          <div className="scope-section-help">
            global = the same value applies across every profile. profile = the value moves with the
            active profile. font covers font-family + font-size as one toggle.
          </div>
          <div className="scope-grid">
            <ScopeRow
              label="theme"
              value={scope.theme}
              onChange={(v) => void handleScopeChange('theme', v)}
            />
            <ScopeRow
              label="font"
              value={scope.font}
              onChange={(v) => void handleScopeChange('font', v)}
            />
            <ScopeRow
              label="dock layout"
              value={scope.dock_layout}
              onChange={(v) => void handleScopeChange('dock_layout', v)}
            />
            <ScopeRow
              label="keep last command"
              value={scope.keep_last_command}
              onChange={(v) => void handleScopeChange('keep_last_command', v)}
            />
            <ScopeRow
              label="auto check updates"
              value={scope.auto_update}
              onChange={(v) => void handleScopeChange('auto_update', v)}
            />
          </div>
        </div>
      )}

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
            duplicating={duplicateTarget === p.name}
            duplicateDraft={duplicateDraft}
            onDuplicateDraft={setDuplicateDraft}
            onBeginDuplicate={() => beginDuplicate(p.name)}
            onCommitDuplicate={() => void commitDuplicate()}
            onCancelDuplicate={cancelDuplicate}
            onSwitch={() => void handleSwitch(p.name)}
            onDelete={() => void handleDelete(p.name)}
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

      <div className="migration-section-row">
        <div className="migration-section-text">
          <div className="migration-section-title-inline">global catalog (preview)</div>
          <div className="migration-section-hint">
            see how your profiles would merge into a single shared catalog with one loadout per
            profile. read-only, nothing is written.
          </div>
        </div>
        <button type="button" className="settings-btn" onClick={() => setShowMigrationWizard(true)}>
          [preview migration]
        </button>
      </div>
      {showMigrationWizard && <MigrationWizard onClose={() => setShowMigrationWizard(false)} />}
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
  duplicating: boolean;
  duplicateDraft: string;
  onDuplicateDraft: (v: string) => void;
  onBeginDuplicate: () => void;
  onCommitDuplicate: () => void;
  onCancelDuplicate: () => void;
  onSwitch: () => void;
  onDelete: () => void;
  onSaveAutoMatch: (
    am: { host: string | null; port: number | null; characters: string[] } | null,
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
  duplicating,
  duplicateDraft,
  onDuplicateDraft,
  onBeginDuplicate,
  onCommitDuplicate,
  onCancelDuplicate,
  onSwitch,
  onDelete,
  onSaveAutoMatch,
}: RowProps) {
  const [open, setOpen] = useState(false);
  const [hostDraft, setHostDraft] = useState(entry.auto_match?.host ?? '');
  const [portDraft, setPortDraft] = useState(
    entry.auto_match?.port ? String(entry.auto_match.port) : '',
  );
  // Comma-separated character list. Free-form display while editing
  // so backspace + retype stays predictable; we split + trim at save
  // time and the backend re-emits as a JSON array. Any-of matching
  // means a profile listing "Erelei, Akletus, Vanek" claims any of
  // those three at connect time.
  const [charDraft, setCharDraft] = useState((entry.auto_match?.characters ?? []).join(', '));
  const [descDraft, setDescDraft] = useState(entry.description ?? '');

  useEffect(() => {
    setHostDraft(entry.auto_match?.host ?? '');
    setPortDraft(entry.auto_match?.port ? String(entry.auto_match.port) : '');
    setCharDraft((entry.auto_match?.characters ?? []).join(', '));
    setDescDraft(entry.description ?? '');
  }, [entry]);

  const handleSaveMatch = () => {
    const host = hostDraft.trim();
    const port = portDraft.trim() ? Number(portDraft.trim()) : null;
    const characters = charDraft
      .split(',')
      .map((c) => c.trim())
      .filter((c) => c.length > 0);
    const description = descDraft.trim();
    const hasAny = host || port !== null || characters.length > 0;
    onSaveAutoMatch(
      hasAny
        ? {
            host: host || null,
            port: port && Number.isFinite(port) ? port : null,
            characters,
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
          {duplicating ? (
            <input
              type="text"
              className="profile-row-rename"
              autoFocus
              spellCheck={false}
              value={duplicateDraft}
              placeholder="new profile name"
              onChange={(e) => onDuplicateDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') onCommitDuplicate();
                if (e.key === 'Escape') onCancelDuplicate();
              }}
              onBlur={onCommitDuplicate}
            />
          ) : (
            <button
              type="button"
              className="settings-btn settings-btn-mute"
              onClick={onBeginDuplicate}
            >
              [duplicate]
            </button>
          )}
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
              <span>characters</span>
              <input
                type="text"
                spellCheck={false}
                value={charDraft}
                placeholder="comma-separated, e.g. Erelei, Akletus, Vanek"
                title="any-of match. leave empty to match this MUD regardless of which character logs in"
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

function ScopeRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: ProfileScope;
  onChange: (v: ProfileScope) => void;
}) {
  return (
    <div className="scope-row">
      <span className="scope-row-label">{label}</span>
      <div className="scope-row-toggle">
        <button
          type="button"
          className={`scope-pill${value === 'global' ? ' is-active' : ''}`}
          aria-pressed={value === 'global'}
          onClick={() => onChange('global')}
        >
          global
        </button>
        <button
          type="button"
          className={`scope-pill${value === 'profile' ? ' is-active' : ''}`}
          aria-pressed={value === 'profile'}
          onClick={() => onChange('profile')}
        >
          profile
        </button>
      </div>
    </div>
  );
}
