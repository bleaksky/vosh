import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { listLogSessions, logsKeepGet, logsKeepSet } from '../../ipc/logs';
import {
  profileGetScope,
  profileSetScope,
  subscribeProfilesChanged,
  type ProfileScope,
  type ScopeConfig,
} from '../../ipc/profiles';
import { checkForUpdate, installUpdateAndRelaunch } from '../../ipc/updater';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import APP_SHORTCUTS from '../../lib/appShortcuts.json';
import { isMacPlatform, shortcutLabel } from '../../lib/shortcuts';
import { isLocalHost, KEEP_LOGS, savedLogsText } from './logView';
import { settingsSubpage } from '../../lib/settingsNav';
import { KNOWN_WORLDS } from '../../lib/knownWorlds';
import { useSessions } from '../../stores/session/sessionsStore';
import { parseTarget, useSessionTarget } from '../../stores/session/useConnection';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import {
  Button,
  Card,
  Disclosure,
  DisclosurePanel,
  Field,
  Row,
  Section,
  Select,
  Toggle,
} from '../../ui';
import { ReconnectRow } from './ReconnectRow';
import { SessionLogs } from './SessionLogs';
import { OTHER, worldChoice, worldValue } from './worldChoice';

// General (the approved SettingsGeneral board): where Connect dials,
// updates, the settings every character shares, and the saved session
// logs. Search logs… opens the log view inside General at
// general:logs (SessionLogs.tsx). Windows and Linux add an Advanced
// disclosure at the end with the GPU rendering switch, which drives
// the xterm renderer macOS does not show.

export function GeneralPage(props: SettingsPageProps) {
  if (settingsSubpage(props.target) !== null) return <SessionLogs {...props} />;
  return <GeneralSections {...props} />;
}

/** What to call this computer in a sentence: Mac, PC, or computer. */
function computerName(): string {
  const platform =
    typeof document !== 'undefined' ? document.documentElement.dataset.platform : undefined;
  if (platform === 'macos' || (!platform && isMacPlatform())) return 'Mac';
  if (platform === 'windows') return 'PC';
  return 'computer';
}

function GeneralSections({
  target,
  navSeq,
  config,
  setConfig,
  onError,
  navigate,
}: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const mac = isMacPlatform();
  return (
    <>
      <ConnectionSection onError={onError} />
      <UpdatesSection
        autoUpdate={config?.auto_update ?? false}
        onAutoUpdate={(on) => update({ auto_update: on })}
        disabled={config === null}
      />
      <ScopeSection onError={onError} />
      <SessionLogsSection
        logSessions={config?.log_sessions ?? null}
        onLogSessions={(on) => update({ log_sessions: on })}
        disabled={config === null}
        onError={onError}
        onSearch={() => navigate({ group: 'general', section: 'logs' })}
      />
      {!mac && <AdvancedSection target={target} navSeq={navSeq} />}
    </>
  );
}

// ── Connection ─────────────────────────────────────────────────────

/** Where Connect and Cmd+R dial the selected session, the same target
 *  the session popover's Edit connection… edits, which each session
 *  keeps for itself (board 7). The World select picks a known world or
 *  Other…, which clears host and port for you to type. Host and port
 *  save when you leave them or press Enter, and go back to the target
 *  when they do not make one. Reconnect when the link drops belongs to
 *  the profile Settings shows, not the session. */
function ConnectionSection({ onError }: { onError: (message: string | null) => void }) {
  const [target, storeTarget] = useSessionTarget();
  const sessions = useSessions().length;
  const [host, setHost] = useState(target.host);
  const [port, setPort] = useState(String(target.port));
  const [other, setOther] = useState(false);
  const hostRef = useRef<HTMLInputElement | null>(null);
  const portId = useId();
  // The newest target. Leaving the host field for the TLS switch
  // saves the host and flips TLS in one gesture, before a render.
  const latest = useRef(target);

  const saveTarget = (next: typeof target) => {
    latest.current = next;
    storeTarget(next);
  };

  // Follow the target, from here, the session popover or a selection.
  useEffect(() => {
    latest.current = target;
    setHost(target.host);
    setPort(String(target.port));
    setOther(false);
  }, [target]);

  useEffect(() => {
    if (other) hostRef.current?.focus();
  }, [other]);

  const revert = () => {
    setHost(latest.current.host);
    setPort(String(latest.current.port));
    setOther(false);
  };

  const commit = () => {
    const saved = latest.current;
    const next = parseTarget({ host, port, tls: saved.tls });
    if (!next) {
      revert();
      return;
    }
    setOther(false);
    if (next.host !== saved.host || next.port !== saved.port) saveTarget(next);
    else setHost(next.host);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      commit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      revert();
    }
  };

  const choice = worldChoice(target);
  const value = other ? OTHER : choice.value;

  const pickWorld = (next: string) => {
    if (next === OTHER) {
      setOther(true);
      setHost('');
      setPort('');
      return;
    }
    const picked = KNOWN_WORLDS.find((w) => worldValue(w) === next);
    if (picked) saveTarget({ host: picked.host, port: picked.port, tls: latest.current.tls });
    else revert();
  };

  return (
    <Section id="connection" title="Connection">
      <Row
        label="World"
        description={`Where Connect and ${shortcutLabel(APP_SHORTCUTS.connect)} take ${sessions > 1 ? 'this session' : 'you'}.`}
        anchor="world"
      >
        <Select value={value} onChange={pickWorld} options={choice.options} width={296} />
      </Row>
      <Row label="Host and port" anchor="host">
        <span
          className="st-field-pair"
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) commit();
          }}
        >
          <Field
            ref={hostRef}
            value={host}
            onChange={setHost}
            onKeyDown={onKeyDown}
            width={224}
            mono
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="mud.example.org"
          />
          <Field
            id={portId}
            aria-label="Port"
            value={port}
            onChange={(v) => setPort(v.replace(/[^0-9]/g, ''))}
            onKeyDown={onKeyDown}
            width={64}
            mono
            inputMode="numeric"
            placeholder="4000"
          />
        </span>
      </Row>
      <Row label="Use TLS" anchor="tls">
        <Toggle checked={target.tls} onChange={(tls) => saveTarget({ ...latest.current, tls })} />
      </Row>
      <ReconnectRow onError={onError} />
    </Section>
  );
}

// ── Updates ────────────────────────────────────────────────────────

type UpdateState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'current' }
  | { kind: 'available'; version: string }
  | { kind: 'installing'; version: string }
  | { kind: 'error'; message: string; detail: string };

function updateHint(state: UpdateState): string {
  switch (state.kind) {
    case 'idle':
      return `You have Vosh ${__APP_VERSION__}.`;
    case 'checking':
      return 'Checking for updates…';
    case 'current':
      return 'Vosh is up to date.';
    case 'available':
      return `Vosh ${state.version} is ready.`;
    case 'installing':
      return `Installing Vosh ${state.version}…`;
    case 'error':
      return state.message;
  }
}

function UpdatesSection({
  autoUpdate,
  onAutoUpdate,
  disabled,
}: {
  autoUpdate: boolean;
  onAutoUpdate: (on: boolean) => void;
  disabled: boolean;
}) {
  const [state, setState] = useState<UpdateState>({ kind: 'idle' });
  const hintId = useId();

  const check = async () => {
    setState({ kind: 'checking' });
    try {
      const result = await checkForUpdate();
      setState(
        result.available
          ? { kind: 'available', version: result.version ?? '' }
          : { kind: 'current' },
      );
    } catch (e) {
      setState({ kind: 'error', message: 'Vosh could not check for updates.', detail: String(e) });
    }
  };

  const install = async (version: string) => {
    setState({ kind: 'installing', version });
    try {
      await installUpdateAndRelaunch();
    } catch (e) {
      setState({ kind: 'error', message: 'Vosh could not install the update.', detail: String(e) });
    }
  };

  const ready = state.kind === 'available' || state.kind === 'installing';
  return (
    <Section
      id="updates"
      title="Updates"
      actions={
        <>
          <span
            id={hintId}
            className="st-meta"
            role="status"
            data-tone={state.kind === 'error' ? 'danger' : undefined}
            title={state.kind === 'error' ? state.detail : undefined}
          >
            {updateHint(state)}
          </span>
          {ready ? (
            <Button
              variant="primary"
              aria-describedby={hintId}
              disabled={state.kind === 'installing'}
              onClick={() => state.kind === 'available' && void install(state.version)}
            >
              Install and restart
            </Button>
          ) : (
            <Button
              aria-describedby={hintId}
              disabled={state.kind === 'checking'}
              onClick={() => void check()}
            >
              Check now
            </Button>
          )}
        </>
      }
    >
      <Row label="Check for updates when Vosh opens" anchor="auto-update">
        <Toggle checked={autoUpdate} onChange={onAutoUpdate} disabled={disabled} />
      </Row>
    </Section>
  );
}

// ── Keep the same for every character ──────────────────────────────

const SCOPE_ROWS: { key: keyof ScopeConfig; label: string }[] = [
  { key: 'theme', label: 'Theme' },
  { key: 'font', label: 'Font and size' },
  { key: 'keep_last_command', label: 'Keep last command' },
  { key: 'auto_update', label: 'Check for updates' },
];

/** The scope categories as switches: on keeps one value for every
 *  character (global), off lets each character keep its own
 *  (profile). The dock layout category stays in the index for older
 *  builds and has no switch. */
function ScopeSection({ onError }: { onError: (message: string | null) => void }) {
  const [scope, setScope] = useState<ScopeConfig | null>(null);

  const reload = useCallback(() => {
    profileGetScope()
      .then(setScope)
      .catch((e) => onError(String(e)));
  }, [onError]);
  useEffect(() => reload(), [reload]);
  useTauriEvent(subscribeProfilesChanged, reload);

  const set = async (key: keyof ScopeConfig, shared: boolean) => {
    if (!scope) return;
    const value: ProfileScope = shared ? 'global' : 'profile';
    const next = { ...scope, [key]: value };
    setScope(next);
    try {
      await profileSetScope(next);
      onError(null);
    } catch (e) {
      setScope(scope);
      onError(String(e));
    }
  };

  return (
    <Section
      id="scope"
      title="Keep the same for every character"
      actions={<span className="st-meta">Turn one off and each character keeps its own.</span>}
      card={false}
    >
      <Card columns>
        {SCOPE_ROWS.map(({ key, label }) => (
          <Row key={key} label={label} anchor={`scope-${key.replace(/_/g, '-')}`}>
            <Toggle
              checked={scope ? scope[key] === 'global' : false}
              disabled={scope === null}
              onChange={(on) => void set(key, on)}
            />
          </Row>
        ))}
      </Card>
    </Section>
  );
}

// ── Session logs ───────────────────────────────────────────────────

/** Saved logs with the way into the log view, Log sessions for the
 *  profile Settings shows, and Keep logs for, which every profile
 *  shares since they share one log file (D34). Log sessions reads the
 *  world the selected session dials until you choose, on for a game and
 *  off for this computer. */
function SessionLogsSection({
  logSessions,
  onLogSessions,
  disabled,
  onError,
  onSearch,
}: {
  logSessions: boolean | null;
  onLogSessions: (on: boolean) => void;
  disabled: boolean;
  onError: (message: string | null) => void;
  onSearch: () => void;
}) {
  const [target] = useSessionTarget();
  const [keep, setKeep] = useState<number | null | undefined>(undefined);
  useEffect(() => {
    let cancelled = false;
    logsKeepGet()
      .then((days) => {
        if (!cancelled) setKeep(days);
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);

  const pickKeep = async (value: string) => {
    const days = value === 'forever' ? null : Number(value);
    const before = keep;
    setKeep(days);
    try {
      await logsKeepSet(days);
      onError(null);
    } catch (e) {
      setKeep(before);
      onError(String(e));
    }
  };

  return (
    <Section
      id="session-logs"
      title="Session logs"
      help={{ topic: 'characters-and-data.search-logs', subject: 'session logs' }}
    >
      <Row label="Saved logs" description={<SavedLogsCount onError={onError} />}>
        <Button onClick={onSearch}>Search logs…</Button>
      </Row>
      <Row
        label="Log sessions"
        description="Saves every line this character's sessions show, so you can search them later. Connections to this computer stay out until you turn it on."
        anchor="log-sessions"
      >
        <Toggle
          checked={logSessions ?? !isLocalHost(target.host)}
          disabled={disabled}
          onChange={onLogSessions}
        />
      </Row>
      <Row
        label="Keep logs for"
        description="Vosh deletes logs older than this once a day. The first time it also tidies the file, and new game text waits until that's done."
        anchor="keep-logs"
      >
        <Select
          value={keep ? String(keep) : 'forever'}
          disabled={keep === undefined}
          options={KEEP_LOGS}
          onChange={(v) => void pickKeep(v)}
        />
      </Row>
    </Section>
  );
}

/** How many logs and lines Vosh saved, leaving out connections to
 *  this machine the way the log view does. A log is one connection,
 *  which the store calls a session (Q21). */
function SavedLogsCount({ onError }: { onError: (message: string | null) => void }) {
  const [counts, setCounts] = useState<{ logs: number; lines: number } | null>(null);
  useEffect(() => {
    let cancelled = false;
    listLogSessions(0, { hideLocal: true })
      .then((rows) => {
        if (cancelled) return;
        setCounts({
          logs: rows.length,
          lines: rows.reduce((sum, row) => sum + row.line_count, 0),
        });
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);
  if (!counts) return 'Counting your saved logs…';
  return savedLogsText(counts.logs, counts.lines, computerName());
}

// ── Advanced (Windows and Linux) ───────────────────────────────────

// GPU rendering lives in browser storage, not the profile, because
// whether WebGL works is a property of this computer. On is the
// default, so on clears the flag and off writes '0'. The terminal
// reads it when it mounts, so a change takes effect after a restart.
const WEBGL_KEY = 'vosh.webgl';

function readGpu(): boolean {
  try {
    return localStorage.getItem(WEBGL_KEY) !== '0';
  } catch {
    return true;
  }
}

function writeGpu(on: boolean): void {
  try {
    if (on) localStorage.removeItem(WEBGL_KEY);
    else localStorage.setItem(WEBGL_KEY, '0');
  } catch {
    // Storage unavailable. The terminal keeps its default.
  }
}

function AdvancedSection({ target, navSeq }: Pick<SettingsPageProps, 'target' | 'navSeq'>) {
  const names = (t: typeof target) => t.section === 'advanced' || t.anchor === 'gpu';
  const [open, setOpen] = useState(() => names(target));
  const [gpu, setGpu] = useState(readGpu);
  const rowsId = useId();

  // A deep link or search hit on the GPU row opens the disclosure.
  useEffect(() => {
    if (names(target)) setOpen(true);
    // navSeq changes on every navigation, even to the same target.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navSeq]);

  return (
    <section className="st-section" aria-label="Advanced" data-st-anchor="advanced">
      <Card>
        <Disclosure
          label="Advanced"
          description="Choose how Vosh draws the terminal on this computer."
          expanded={open}
          aria-controls={rowsId}
          onClick={() => setOpen((v) => !v)}
        />
        {open && (
          <DisclosurePanel id={rowsId}>
            <Row
              label="GPU rendering"
              description="Vosh draws the terminal with your graphics card. Restart Vosh after you change this."
              anchor="gpu"
            >
              <Toggle
                checked={gpu}
                onChange={(on) => {
                  setGpu(on);
                  writeGpu(on);
                }}
              />
            </Row>
          </DisclosurePanel>
        )}
      </Card>
    </section>
  );
}
