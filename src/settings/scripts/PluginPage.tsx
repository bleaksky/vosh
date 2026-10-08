import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import {
  pluginOwner,
  pluginRead,
  pluginSave,
  type LuaLine,
  type PluginFolder,
  type PluginManifest,
  type PluginRow,
} from '../../ipc/scripts';
import { errorText } from '../../lib/text';
import { Button, Card, CardNote, Segmented, Toggle, cx, type SegmentedOption } from '../../ui';
import { CodeEditor } from '../../ui/CodeEditor';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import type { LeaveGuard } from '../pageTypes';
import { useCloseGuard } from '../useCloseGuard';
import { LuaConsole } from './LuaConsole';
import { ManifestCard } from './ManifestCard';
import { errorMark, saveStatus, stopNote, type PluginSave } from './pluginState';
import { switchPlugin } from './switchPlugin';

// A plugin's own page under Scripts. The toolbar switches between the
// file the plugin runs first and its Manifest, with On for this profile
// at its right. Under the file sit the stop note while Vosh holds the
// plugin off, the editor, and Output with this plugin's lines and a
// console that runs inside it. Both tabs edit one draft, and the save
// bar writes it. Leaving the page, by another group or the crumb back
// to Scripts, or closing the window with unsaved changes asks first, as
// Automation does.

type Tab = 'code' | 'manifest';

/** What the page edits: the manifest, the code of the file Runs first
 *  names, and that file's code as it is on disk, so a pick under Runs
 *  first knows whether it drops an edit. */
interface Draft {
  manifest: PluginManifest;
  code: string;
  read: string;
}

/** A question the page asks before it drops your edits. */
interface Asking {
  title: string;
  proceed: () => void;
}

const NO_MARKS: readonly never[] = [];

/** Whether two manifests say the same, the name aside, which is the
 *  folder's. */
function sameManifest(a: PluginManifest, b: PluginManifest): boolean {
  return (
    a.version === b.version &&
    a.author === b.author &&
    a.description === b.description &&
    a.entry === b.entry
  );
}

interface Props {
  name: string;
  /** The plugins as the selected session sees them, or null until the
   *  list arrives. */
  plugins: PluginRow[] | null;
  /** Every line of the session's Output ring. */
  lines: LuaLine[];
  /** The last save of this plugin in this window. */
  saved: PluginSave | undefined;
  onSaved: (save: PluginSave) => void;
  /** Show the list a change handed back. */
  onPlugins: (plugins: PluginRow[]) => void;
  /** Read the list again, after a change that failed. */
  onChanged: () => void;
  /** Let go of this plugin's lines once Clear empties them. */
  onCleared: () => void;
  onError: (message: string | null) => void;
  setLeaveGuard: (guard: LeaveGuard | null) => void;
}

export function PluginPage({
  name,
  plugins,
  lines,
  saved,
  onSaved,
  onPlugins,
  onChanged,
  onCleared,
  onError,
  setLeaveGuard,
}: Props) {
  const switchId = useId();
  const [tab, setTab] = useState<Tab>('code');
  // The folder as Vosh read it, and as the last save left it.
  const [file, setFile] = useState<PluginFolder | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState<Asking | null>(null);

  useEffect(() => {
    pluginRead(name)
      .then((read) => {
        setFile(read);
        setDraft({ manifest: read.manifest, code: read.code, read: read.code });
      })
      .catch((e: unknown) => onError(errorText(e)));
  }, [name, onError]);

  const dirty =
    file !== null &&
    draft !== null &&
    (draft.code !== file.code || !sameManifest(draft.manifest, file.manifest));
  const dirtyRef = useRef(dirty);
  useEffect(() => {
    dirtyRef.current = dirty;
  }, [dirty]);

  /** Run `proceed` now, or after you agree to drop unsaved changes. */
  const ask = useCallback(
    (proceed: () => void) => {
      if (!dirtyRef.current) proceed();
      else setAsking({ title: `Discard changes to ${name}?`, proceed });
    },
    [name],
  );

  // Moving to another group or back to the list asks first.
  useEffect(() => {
    setLeaveGuard((proceed) => {
      if (!dirtyRef.current) return false;
      ask(proceed);
      return true;
    });
    return () => setLeaveGuard(null);
  }, [setLeaveGuard, ask]);

  // So does closing the window.
  useCloseGuard(dirty, ask);

  const row = plugins?.find((p) => p.name === name);
  const entry = draft?.manifest.entry ?? '';
  const mine = useMemo(
    () => lines.filter((line) => line.owner === pluginOwner(name)),
    [lines, name],
  );
  const loadedMs = row?.loaded_ms ?? null;
  const marks = useMemo(() => {
    const mark = errorMark(lines, name, entry, loadedMs);
    return mark ? [mark] : NO_MARKS;
  }, [lines, name, entry, loadedMs]);

  if (!file || !draft) return null;

  const discard = () => setDraft({ manifest: file.manifest, code: file.code, read: file.code });

  const save = () => {
    const sent = draft;
    setBusy(true);
    pluginSave(name, sent.manifest, sent.code)
      .then((list) => {
        onPlugins(list);
        setFile({ ...file, manifest: { ...sent.manifest, name }, code: sent.code });
        setDraft((now) =>
          now && now.manifest.entry === sent.manifest.entry ? { ...now, read: sent.code } : now,
        );
        onSaved({ at: Date.now(), reloaded: list.some((p) => p.name === name && p.on) });
        onError(null);
      })
      .catch((e: unknown) => onError(errorText(e)))
      .finally(() => setBusy(false));
  };

  // A pick under Runs first opens that file in the editor, after asking
  // when the code in the editor holds an edit it would drop.
  const pickEntry = (next: string) => {
    if (next === draft.manifest.entry) return;
    const open = (code: string) =>
      setDraft((now) => now && { manifest: { ...now.manifest, entry: next }, code, read: code });
    const proceed = () => {
      if (next === file.manifest.entry) open(file.code);
      else
        pluginRead(name, next)
          .then((read) => open(read.code))
          .catch((e: unknown) => onError(errorText(e)));
    };
    if (draft.code === draft.read) proceed();
    else setAsking({ title: `Discard changes to ${draft.manifest.entry}?`, proceed });
  };

  const tabs: readonly SegmentedOption<Tab>[] = [
    { value: 'code', label: entry },
    { value: 'manifest', label: 'Manifest' },
  ];
  const stopped = row?.stopped ?? null;
  const status = dirty ? 'Unsaved changes' : saved ? saveStatus(saved) : '';

  return (
    <div className="st-plugin-page">
      <div className="st-toolbar">
        <Segmented label="File" options={tabs} value={tab} onChange={setTab} />
        <div className="st-control-group">
          <label htmlFor={switchId} className="st-row-label">
            On for this profile
          </label>
          <Toggle
            id={switchId}
            checked={row?.on ?? false}
            disabled={!row}
            onChange={(on) => switchPlugin(plugins, name, on, { onPlugins, onError, onChanged })}
          />
        </div>
      </div>
      {tab === 'code' ? (
        <div className="st-plugin-body">
          {stopped && (
            <Card>
              <CardNote tone="warn">{stopNote(name, stopped)}</CardNote>
            </Card>
          )}
          <CodeEditor
            className={cx('st-code', 'st-plugin-code', stopped && 'is-short')}
            ariaLabel={entry}
            page
            maxHeight="none"
            language="lua"
            value={draft.code}
            onChange={(code) => setDraft((now) => now && { ...now, code })}
            marks={marks}
          />
          <LuaConsole plugin={name} lines={mine} onCleared={onCleared} onError={onError} />
        </div>
      ) : (
        <div className="st-plugin-body st-plugin-manifest">
          <ManifestCard
            name={name}
            manifest={draft.manifest}
            files={file.files}
            folder={file.folder}
            onChange={(patch) =>
              setDraft((now) => now && { ...now, manifest: { ...now.manifest, ...patch } })
            }
            onEntry={pickEntry}
            onError={onError}
          />
        </div>
      )}
      <div className="st-savebar">
        <div className="st-savebar-side">
          <span className="st-savebar-status" role="status" aria-live="polite">
            {status}
          </span>
        </div>
        <div className="st-savebar-actions">
          <Button onClick={discard} disabled={!dirty || busy}>
            Discard
          </Button>
          <Button variant="primary" onClick={save} disabled={!dirty || busy}>
            Save and reload
          </Button>
        </div>
      </div>
      {asking && (
        <ConfirmDialog
          title={asking.title}
          body="Vosh keeps what you saved last."
          confirmLabel="Discard"
          onConfirm={() => {
            setAsking(null);
            asking.proceed();
          }}
          onCancel={() => setAsking(null)}
        />
      )}
    </div>
  );
}
