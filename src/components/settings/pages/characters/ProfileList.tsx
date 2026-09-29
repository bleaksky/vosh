import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import { ConfirmDialog } from '../../../ConfirmDialog';
import {
  MenuItem,
  MenuSeparator,
  MenuSurface,
  type MenuCloseReason,
  type MenuPlacement,
} from '../../../panel/MenuSurface';
import { useEscape } from '../../../../lib/escapeStack';
import {
  copyName,
  movedSentence,
  newProfileClaim,
  newProfileName,
  profileDisplayName,
  profileWorldName,
} from '../../../../lib/characterProfiles';
import {
  profileCreate,
  profileDelete,
  profileDuplicate,
  profileExportFile,
  profileRename,
  profileSetLogin,
  profileSwitch,
  type ProfilesList,
  type SessionIdentity,
} from '../../../../lib/session';
import { Button, Field, IconButton, MoreIcon, PlusIcon, VisuallyHidden, cx } from '../../ui';

// The profile list on the Characters board: one 38 px row per profile
// in index order, the profile in use marked by an accent dot, its world
// as quiet meta, and New profile under it. Selecting a row only shows
// that profile. It never switches the live session. Each row has a
// more menu (SPEC 7) that switches, renames, duplicates, exports, and
// deletes, and a quiet line under the list says what the last action
// did when that is not plain to see.

interface Props {
  list: ProfilesList;
  selected: string | null;
  identity: SessionIdentity | null;
  /** The sentence under the list, or null for none. */
  status: string | null;
  onSelect: (name: string) => void;
  /** A profile to select once the list shows it, after a create, a
   *  copy, or a rename. */
  onShow: (name: string) => void;
  onStatus: (sentence: string | null) => void;
  onError: (message: string | null) => void;
  /** Read the list again after an edit, ahead of the backend's event. */
  onChanged: () => void;
}

type Editing =
  | { kind: 'new' }
  | { kind: 'rename'; name: string }
  | { kind: 'duplicate'; name: string };

interface OpenMenu {
  name: string;
  at: MenuPlacement;
  /** The more button that opened it, which takes focus back. */
  anchor: HTMLButtonElement | null;
}

export function ProfileList({
  list,
  selected,
  identity,
  status,
  onSelect,
  onShow,
  onStatus,
  onError,
  onChanged,
}: Props) {
  const [editing, setEditing] = useState<Editing | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);
  const names = list.profiles.map((p) => p.name);

  const fail = (e: unknown) => onError(String(e));

  const run = async (action: () => Promise<void>) => {
    try {
      await action();
      onError(null);
    } catch (e) {
      fail(e);
    } finally {
      onChanged();
    }
  };

  const openMenu = (name: string, button: HTMLButtonElement | null, point?: MenuPlacement) => {
    let at = point;
    if (!at && button) {
      const r = button.getBoundingClientRect();
      at = { x: r.left, y: r.bottom + 4, flipX: r.right, flipY: r.top - 4 };
    }
    if (at) setMenu({ name, at, anchor: button });
  };

  const closeMenu = (reason: MenuCloseReason | 'select') => {
    const anchor = menu?.anchor;
    setMenu(null);
    if (reason === 'escape') anchor?.focus();
  };

  // Focus goes back to the row's more button, unless the action opens
  // a field that takes it.
  const pick = (action: () => void) => () => {
    const anchor = menu?.anchor;
    closeMenu('select');
    anchor?.focus();
    action();
  };

  const createProfile = async (raw: string): Promise<boolean> => {
    const name = raw.trim();
    if (name.length === 0) {
      setEditing(null);
      return true;
    }
    try {
      const active = list.profiles.find((p) => p.name === list.active);
      const claim = newProfileClaim(identity, active);
      const entry = await profileCreate(name, list.active, claim);
      setEditing(null);
      onShow(entry.name);
      const character = claim?.characters?.[0];
      let sentence: string | null = null;
      if (character) {
        const result = await profileSetLogin(entry.name, character, true);
        sentence = movedSentence(character, result.released_from, entry.name);
      }
      onStatus(sentence);
      onError(null);
      return true;
    } catch (e) {
      fail(e);
      return false;
    } finally {
      onChanged();
    }
  };

  const renameProfile = async (from: string, raw: string): Promise<boolean> => {
    const name = raw.trim();
    if (name.length === 0 || name === from) {
      setEditing(null);
      return true;
    }
    try {
      await profileRename(from, name);
      setEditing(null);
      if (selected === from) onShow(name);
      onError(null);
      return true;
    } catch (e) {
      fail(e);
      return false;
    } finally {
      onChanged();
    }
  };

  const duplicateProfile = async (from: string, raw: string): Promise<boolean> => {
    const name = raw.trim();
    if (name.length === 0) {
      setEditing(null);
      return true;
    }
    try {
      await profileDuplicate(from, name);
      setEditing(null);
      onShow(name);
      onError(null);
      return true;
    } catch (e) {
      fail(e);
      return false;
    } finally {
      onChanged();
    }
  };

  const exportProfile = (name: string) =>
    void run(async () => {
      const saved = await profileExportFile(name);
      onStatus(`Vosh saved ${saved.file_name} in your Downloads folder.`);
    });

  const deleteProfile = (name: string) => {
    setDeleting(null);
    // Show the profile in use instead of one about to go.
    if (selected === name) onSelect(list.active);
    void run(async () => {
      await profileDelete(name);
      onStatus(null);
    });
  };

  // Up and Down move through the rows the way a list does.
  const onRowKey = (e: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const rows = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>('.st-profile-select') ?? [],
    );
    const at = rows.indexOf(e.currentTarget);
    const next = rows[at + (e.key === 'ArrowDown' ? 1 : -1)];
    if (!next) return;
    e.preventDefault();
    next.focus();
    next.click();
  };

  const menuEntry = menu ? list.profiles.find((p) => p.name === menu.name) : undefined;
  const menuActive = menu?.name === list.active;
  const deletingLabel = deleting ? profileDisplayName(deleting) : '';

  return (
    <div className="st-chars-list">
      <ul ref={listRef} className="st-profiles" aria-label="Profiles">
        {list.profiles.map((entry) => {
          const { name } = entry;
          const display = profileDisplayName(name);
          const active = name === list.active;
          const world = profileWorldName(entry);
          const renaming = editing?.kind === 'rename' && editing.name === name;
          const open = menu?.name === name;
          return (
            <Fragment key={name}>
              <li
                className={cx('st-profile', open && 'is-open')}
                onContextMenu={(e) => {
                  if (renaming) return;
                  e.preventDefault();
                  openMenu(name, null, { x: e.clientX, y: e.clientY });
                }}
              >
                {renaming ? (
                  <NameField
                    initial={name}
                    label={`New name for ${display}`}
                    commitOnBlur
                    onCommit={(value) => renameProfile(name, value)}
                    onCancel={() => setEditing(null)}
                  />
                ) : (
                  <>
                    <button
                      type="button"
                      className="st-profile-select"
                      aria-current={selected === name ? 'true' : undefined}
                      onClick={() => onSelect(name)}
                      onKeyDown={onRowKey}
                    >
                      <span
                        className={cx('st-profile-dot', active && 'is-active')}
                        aria-hidden="true"
                      />
                      <span className="st-profile-name">
                        {display}
                        {active && <VisuallyHidden> (in use)</VisuallyHidden>}
                      </span>
                      {world && <span className="st-profile-meta">{world}</span>}
                    </button>
                    <IconButton
                      className="st-profile-more"
                      label={`${display} options`}
                      icon={<MoreIcon />}
                      aria-haspopup="menu"
                      aria-expanded={open}
                      onClick={(e) => {
                        if (open) setMenu(null);
                        else openMenu(name, e.currentTarget);
                      }}
                    />
                  </>
                )}
              </li>
              {editing?.kind === 'duplicate' && editing.name === name && (
                <li className="st-profile">
                  <NameField
                    initial={copyName(name, names)}
                    label={`Name for the copy of ${display}`}
                    onCommit={(value) => duplicateProfile(name, value)}
                    onCancel={() => setEditing(null)}
                  />
                </li>
              )}
            </Fragment>
          );
        })}
        {editing?.kind === 'new' && (
          <li className="st-profile">
            <NameField
              initial={newProfileName(identity, names)}
              label="New profile name"
              onCommit={createProfile}
              onCancel={() => setEditing(null)}
            />
          </li>
        )}
      </ul>

      <Button
        className="st-chars-new"
        icon={<PlusIcon />}
        data-st-anchor="new-profile"
        data-st-flash=""
        disabled={editing?.kind === 'new'}
        onClick={() => setEditing({ kind: 'new' })}
      >
        New profile
      </Button>

      <p className="st-chars-status" role="status">
        {status}
      </p>

      {menu && menuEntry && (
        <MenuSurface
          label={`${profileDisplayName(menu.name)} options`}
          anchor={menu.anchor}
          at={menu.at}
          className="st-profile-menu"
          onClose={closeMenu}
        >
          <MenuItem
            disabled={menuActive}
            onSelect={pick(() => void run(() => profileSwitch(menu.name)))}
          >
            Switch to this profile
          </MenuItem>
          <MenuItem onSelect={pick(() => setEditing({ kind: 'rename', name: menu.name }))}>
            Rename…
          </MenuItem>
          <MenuItem onSelect={pick(() => setEditing({ kind: 'duplicate', name: menu.name }))}>
            Duplicate…
          </MenuItem>
          <MenuItem onSelect={pick(() => exportProfile(menu.name))}>Export to Downloads</MenuItem>
          <MenuSeparator />
          <MenuItem disabled={menuActive} onSelect={pick(() => setDeleting(menu.name))}>
            <span className={menuActive ? undefined : 'st-menu-danger'}>Delete…</span>
          </MenuItem>
        </MenuSurface>
      )}

      {deleting && (
        <ConfirmDialog
          title={`Delete ${deletingLabel}?`}
          body={`Vosh deletes ${deletingLabel} and everything saved in it. You cannot undo this.`}
          confirmLabel="Delete"
          onConfirm={() => deleteProfile(deleting)}
          onCancel={() => setDeleting(null)}
        />
      )}
    </div>
  );
}

interface NameFieldProps {
  initial: string;
  label: string;
  /** Leaving the field saves, the way a rename in Finder does. A new
   *  name waits for Return instead. */
  commitOnBlur?: boolean;
  /** Resolves true when the field can close. */
  onCommit: (value: string) => Promise<boolean>;
  onCancel: () => void;
}

/** A profile name typed in place of a row. Return saves, Esc cancels,
 *  and leaving an empty field cancels. A name the backend refuses keeps
 *  the field open with the error above the page. */
function NameField({ initial, label, commitOnBlur = false, onCommit, onCancel }: NameFieldProps) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement | null>(null);
  // Closed for good, or waiting on the backend. Refs, so a blur right
  // after Return sees the save already under way.
  const done = useRef(false);
  const saving = useRef(false);

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  const cancel = () => {
    if (done.current || saving.current) return;
    done.current = true;
    onCancel();
  };

  useEscape(true, cancel);

  const commit = () => {
    if (done.current || saving.current) return;
    saving.current = true;
    void onCommit(value).then((closed) => {
      saving.current = false;
      if (closed) done.current = true;
    });
  };

  return (
    <div className="st-profile-edit">
      <Field
        ref={ref}
        value={value}
        onChange={setValue}
        width="100%"
        aria-label={label}
        placeholder="Profile name"
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit();
          }
        }}
        onBlur={() => {
          if (value.trim().length === 0) cancel();
          else if (commitOnBlur) commit();
        }}
      />
    </div>
  );
}
