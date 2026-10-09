import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import {
  MenuItem,
  MenuSeparator,
  MenuSurface,
  type MenuCloseReason,
  type MenuPlacement,
} from '../../ui/MenuSurface';
import { menuBelow } from '../../ui/menuPlacement';
import { useEscape } from '../../lib/escapeStack';
import {
  copyName,
  exportCharacters,
  keepsProfileName,
  loginSentence,
  newProfileClaim,
  newProfileName,
  playedProfiles,
  profileDisplayName,
  profileWorld,
  sessionsSentence,
  takenProfileName,
  takenSentence,
} from '../../lib/characterProfiles';
import {
  profileExportFile,
  profileImportRead,
  profileSetLogin,
  type SessionIdentity,
} from '../../ipc/characters';
import {
  profileCreate,
  profileDelete,
  profileDuplicate,
  profileRename,
  profileSwitch,
  type ProfilesList,
} from '../../ipc/profiles';
import { errorText } from '../../lib/text';
import { useSelected, useSessions } from '../../stores/session/sessionsStore';
import { Button, Field, IconButton, MoreIcon, PlusIcon, VisuallyHidden, cx } from '../../ui';
import { ExportDialog } from './ExportDialog';
import type { ImportFile } from './profileImport';

// The profile list on the Characters page: one 38 px row per profile
// in index order, every profile a session plays marked by an accent
// dot, its world as quiet meta, and New profile under it. Selecting a
// row only shows that profile. It never switches a session. Each row
// has a more menu that switches the selected session, renames,
// duplicates, exports, and deletes. A quiet line under the list says
// what the last action did when that is not plain to see, and
// otherwise, with two or more sessions open, which sessions play each
// profile. Export to Downloads first asks which characters the file
// names, when the profile has any. Import… beside New profile reads a
// Vosh profile export you pick, and the page shows its sheet. A file
// that is no export reads as such on the line under the list.

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
  /** A profile export you picked, read, for the import sheet. */
  onImport: (file: ImportFile) => void;
}

type Editing =
  | { kind: 'new' }
  | { kind: 'rename'; name: string }
  | { kind: 'duplicate'; name: string };

/** Where focus goes once a field or the delete dialog closes: a row,
 *  once the list shows it, or New profile. `always` moves it even from
 *  a control, for the dialog, which hands focus back to the row it
 *  deletes. */
type FocusTarget = { row: string; always?: boolean } | 'new';

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
  onImport,
}: Props) {
  const [editing, setEditing] = useState<Editing | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [exporting, setExporting] = useState<{ name: string; characters: string[] } | null>(null);
  const [focusTarget, setFocusTarget] = useState<FocusTarget | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);
  const newRef = useRef<HTMLButtonElement | null>(null);
  const fileRef = useRef<HTMLInputElement | null>(null);
  const names = list.profiles.map((p) => p.name);
  const sessions = useSessions();
  const selectedSession = useSelected();
  const played = playedProfiles(sessions, selectedSession, list.active);

  // A field or the dialog that held focus is gone, so focus would sit on
  // the page. Hand it to the row the action left you on, once the list
  // shows that row, or to New profile. Focus you moved to a control
  // meanwhile stays where you put it.
  useEffect(() => {
    if (!focusTarget) return;
    const current = document.activeElement;
    const lost = !current || current === document.body;
    if (!lost && !(focusTarget !== 'new' && focusTarget.always)) {
      setFocusTarget(null);
      return;
    }
    const target =
      focusTarget === 'new'
        ? newRef.current
        : Array.from(
            listRef.current?.querySelectorAll<HTMLButtonElement>('.st-profile-select') ?? [],
          ).find((b) => b.dataset.profile === focusTarget.row);
    if (!target) return;
    target.focus();
    setFocusTarget(null);
  }, [focusTarget, list, editing]);

  const fail = (e: unknown) => onError(String(e));

  /** Keep the field open with a sentence when another profile has the
   *  name in any case. The backend refuses it too, since the disk
   *  ignores case and the file would belong to that profile. */
  const refuseTaken = (typed: string, renaming?: string): boolean => {
    const taken = takenProfileName(names, typed, renaming);
    if (taken) onError(takenSentence(taken));
    return taken !== null;
  };

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
    const at = point ?? (button && menuBelow(button.getBoundingClientRect()));
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

  // Each field hands `hasFocus`, true while it still holds focus, so a
  // field you close with Return or Esc passes focus on, and one you
  // leave keeps focus where you moved it.
  const closeField = (hasFocus: boolean, target: FocusTarget) => {
    setEditing(null);
    if (hasFocus) setFocusTarget(target);
  };

  const createProfile = async (raw: string, hasFocus: () => boolean): Promise<boolean> => {
    const name = raw.trim();
    if (name.length === 0) {
      closeField(hasFocus(), 'new');
      return true;
    }
    if (refuseTaken(name)) return false;
    try {
      const active = list.profiles.find((p) => p.name === list.active);
      const claim = newProfileClaim(identity, active);
      const entry = await profileCreate(name, list.active, claim);
      closeField(hasFocus(), { row: entry.name });
      onShow(entry.name);
      const character = claim?.characters?.[0];
      let sentence: string | null = null;
      if (character) {
        const result = await profileSetLogin(entry.name, character, true);
        sentence = loginSentence(character, result, entry.name);
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

  const renameProfile = async (
    from: string,
    raw: string,
    hasFocus: () => boolean,
  ): Promise<boolean> => {
    const name = raw.trim();
    if (name.length === 0 || keepsProfileName(from, name)) {
      closeField(hasFocus(), { row: from });
      return true;
    }
    if (refuseTaken(name, from)) return false;
    try {
      await profileRename(from, name);
      closeField(hasFocus(), { row: name });
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

  const duplicateProfile = async (
    from: string,
    raw: string,
    hasFocus: () => boolean,
  ): Promise<boolean> => {
    const name = raw.trim();
    if (name.length === 0) {
      closeField(hasFocus(), { row: from });
      return true;
    }
    if (refuseTaken(name)) return false;
    try {
      await profileDuplicate(from, name);
      closeField(hasFocus(), { row: name });
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

  // Read the file you picked. A refusal, like a file that is no
  // export, reads on the line under the list. A file the window cannot
  // read gets the sentence Vosh gives one it cannot parse.
  const importFile = async (file: File) => {
    onError(null);
    let text: string;
    try {
      text = await file.text();
    } catch {
      onStatus(`Vosh could not read ${file.name}.`);
      return;
    }
    try {
      const preview = await profileImportRead(file.name, text);
      onImport({ fileName: file.name, text, preview });
    } catch (e) {
      onStatus(errorText(e));
    }
  };

  const saveExport = (name: string, characters: string[]) =>
    void run(async () => {
      const saved = await profileExportFile(name, characters);
      onStatus(`Vosh saved ${saved.file_name} in your Downloads folder.`);
    });

  // A profile with characters on its world asks which ones the file
  // names. One with none exports at once.
  const exportProfile = (name: string) => {
    const characters = exportCharacters(list.profiles.find((p) => p.name === name));
    if (characters.length === 0) saveExport(name, []);
    else setExporting({ name, characters });
  };

  const deleteProfile = (name: string) => {
    setDeleting(null);
    // Show the profile in use instead of one about to go.
    const shown = selected === name || !selected ? list.active : selected;
    if (shown !== selected) onSelect(shown);
    // The dialog hands focus back to the more button of the row that
    // is going, so focus moves on to the row you see.
    setFocusTarget({ row: shown, always: true });
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
  // Switch dims only for the profile the selected session plays, since
  // another session's profile stays open to it, and Delete refuses every
  // profile a session plays.
  const menuSelected = menu?.name === played.selected;
  const menuPlayed = menu ? played.all.has(menu.name) : false;
  const deletingLabel = deleting ? profileDisplayName(deleting) : '';

  return (
    <div className="st-chars-list">
      <ul ref={listRef} className="st-profiles" aria-label="Profiles">
        {list.profiles.map((entry) => {
          const { name } = entry;
          const display = profileDisplayName(name);
          const plays = played.all.has(name);
          const world = profileWorld(entry);
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
                    initial={display}
                    label={`New name for ${display}`}
                    commitOnBlur
                    onCommit={(value, hasFocus) => renameProfile(name, value, hasFocus)}
                    onCancel={(hasFocus) => closeField(hasFocus, { row: name })}
                  />
                ) : (
                  <>
                    <button
                      type="button"
                      className="st-profile-select"
                      data-profile={name}
                      aria-current={selected === name ? 'true' : undefined}
                      onClick={() => onSelect(name)}
                      onKeyDown={onRowKey}
                    >
                      <span
                        className={cx('st-profile-dot dot', plays && 'is-accent')}
                        aria-hidden="true"
                      />
                      <span className="st-profile-name">
                        {display}
                        {plays && <VisuallyHidden> (in use)</VisuallyHidden>}
                      </span>
                      {world && (
                        <span className="st-profile-meta">
                          <span className="st-profile-world">{world.world}</span>
                          {world.port}
                        </span>
                      )}
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
                    onCommit={(value, hasFocus) => duplicateProfile(name, value, hasFocus)}
                    onCancel={(hasFocus) => closeField(hasFocus, { row: name })}
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
              onCancel={(hasFocus) => closeField(hasFocus, 'new')}
            />
          </li>
        )}
      </ul>

      <div className="st-chars-actions">
        <Button
          ref={newRef}
          icon={<PlusIcon />}
          data-st-anchor="new-profile"
          data-st-flash=""
          disabled={editing?.kind === 'new'}
          onClick={() => setEditing({ kind: 'new' })}
        >
          New profile
        </Button>
        <Button
          data-st-anchor="import-profile"
          data-st-flash=""
          onClick={() => fileRef.current?.click()}
        >
          Import…
        </Button>
        <input
          ref={fileRef}
          type="file"
          accept=".toml"
          className="visually-hidden"
          tabIndex={-1}
          aria-hidden="true"
          onChange={(e) => {
            const file = e.target.files?.[0];
            // Clear the pick so the same file can be picked again.
            e.target.value = '';
            if (file) void importFile(file);
          }}
        />
      </div>

      <p className="st-chars-status" role="status">
        {status ?? sessionsSentence(list.profiles, sessions)}
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
            disabled={menuSelected}
            onSelect={pick(() => void run(() => profileSwitch(menu.name, selectedSession)))}
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
          <MenuItem disabled={menuPlayed} onSelect={pick(() => setDeleting(menu.name))}>
            <span className={menuPlayed ? undefined : 'st-menu-danger'}>Delete…</span>
          </MenuItem>
        </MenuSurface>
      )}

      {exporting && (
        <ExportDialog
          name={exporting.name}
          characters={exporting.characters}
          onExport={(characters) => {
            setExporting(null);
            saveExport(exporting.name, characters);
          }}
          onCancel={() => setExporting(null)}
        />
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
  /** Resolves true when the field can close. `hasFocus` reads true
   *  while the field still holds focus, as after Return. */
  onCommit: (value: string, hasFocus: () => boolean) => Promise<boolean>;
  /** `hasFocus` is true for Esc pressed in the field, and false when
   *  leaving it empty closed it. */
  onCancel: (hasFocus: boolean) => void;
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
  const focused = useRef(false);
  const hasFocus = () => focused.current;

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  const cancel = () => {
    if (done.current || saving.current) return;
    done.current = true;
    onCancel(focused.current);
  };

  useEscape(true, cancel);

  const commit = () => {
    if (done.current || saving.current) return;
    saving.current = true;
    void onCommit(value, hasFocus).then((closed) => {
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
        onFocus={() => {
          focused.current = true;
        }}
        onBlur={() => {
          focused.current = false;
          if (value.trim().length === 0) cancel();
          else if (commitOnBlur) commit();
        }}
      />
    </div>
  );
}
