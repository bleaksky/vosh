import { useEffect, useRef, useState } from 'react';
import { profileSwitch, profilesList, type ProfileEntry } from '../ipc/profiles';
import type { ConnectionTarget } from '../ipc/session';
import type { OpenedSession } from '../lib/appMenu';
import { profileDisplayName } from '../lib/characterProfiles';
import { pickProfile, profileLines, showsProfileRow } from '../lib/sessionProfile';
import { useSessions } from '../stores/session/sessionsStore';
import {
  loadTarget,
  profileSwitchErrorMessage,
  type Connection,
} from '../stores/session/useConnection';
import { pushToast } from '../stores/toasts';
import { Select } from '../ui/Select';
import { ConnectionForm } from './ConnectionForm';
import { cancelNewSession } from './newSession';

// The New session form of board 4, for a session New session… opened.
// Host and port start from the saved world, and the port takes the
// caret. The Profile row starts on the profile the session opened on and
// picks again as you edit the address, until you choose one yourself.
// Each new pick moves the session to that profile, so the window takes
// its layout. Connect dials on the profile the form shows. The form
// closes its session again when it goes any other way, by Cancel, Esc or
// a press outside, and writes nothing.

/** How long an edit to the address rests before the form picks again. */
const REPICK_MS = 300;

interface Props {
  opened: OpenedSession;
  connection: Connection;
  /** Close the popover. */
  onClose: () => void;
}

export function NewSessionForm({ opened, connection, onClose }: Props) {
  const [initial] = useState(loadTarget);
  const [address, setAddress] = useState<ConnectionTarget | null>(initial);
  const [profiles, setProfiles] = useState<ProfileEntry[] | null>(null);
  const [pick, setPick] = useState(opened.profile);
  const rows = useSessions();
  // The newest pick, whether you chose it, and the switches in flight,
  // read by Connect after the render that set them.
  const pickRef = useRef(opened.profile);
  const chosen = useRef(false);
  const switching = useRef<Promise<void>>(Promise.resolve());
  const dialed = useRef(false);

  useEffect(() => {
    let live = true;
    profilesList()
      .then((list) => live && setProfiles(list.profiles))
      .catch((e: unknown) => console.warn('[new session] profiles_list', e));
    return () => {
      live = false;
    };
  }, []);

  // A form that goes without dialing closes its session.
  useEffect(
    () => () => {
      if (!dialed.current) void cancelNewSession(opened);
    },
    [opened],
  );

  /** Move the session to `name`. A switch that fails puts the pick back
   *  and says why. */
  const choose = (name: string) => {
    const was = pickRef.current;
    if (name === was) return;
    pickRef.current = name;
    setPick(name);
    switching.current = switching.current.then(() =>
      profileSwitch(name, opened.id).catch((e: unknown) => {
        pushToast({ kind: 'error', message: profileSwitchErrorMessage(e) });
        if (pickRef.current === name) {
          pickRef.current = was;
          setPick(was);
        }
      }),
    );
  };

  /** The pick for `to`, while you have not chosen one. */
  const repick = (to: ConnectionTarget) => {
    if (!chosen.current && profiles) choose(pickProfile(profiles, to.host, to.port, opened.front));
  };

  const host = address?.host;
  const port = address?.port;
  useEffect(() => {
    if (host === undefined || port === undefined) return;
    const timer = window.setTimeout(() => repick({ host, port, tls: false }), REPICK_MS);
    return () => window.clearTimeout(timer);
    // The pick follows the address and the profile list alone.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host, port, profiles]);

  const connect = async (target: ConnectionTarget) => {
    dialed.current = true;
    onClose();
    repick(target);
    await switching.current;
    await connection.connectNew(target, opened.id);
  };

  const shown = profiles !== null && showsProfileRow(profiles, rows, opened.id, pick);
  const lines =
    profiles && address
      ? profileLines(profiles, rows, opened.id, pick, address.host, address.port)
      : { hint: null, warn: null };
  const names = (profiles ?? []).map((p) => p.name);
  if (!names.includes(pick)) names.push(pick);
  const options = names.map((name) => ({ value: name, label: profileDisplayName(name) }));

  return (
    <ConnectionForm
      title="New session"
      submitLabel="Connect"
      initial={initial}
      focusPort
      onEdit={setAddress}
      onCancel={onClose}
      onSubmit={(target) => void connect(target)}
    >
      {shown && (
        <label className="shell-field st-controls">
          <span className="shell-field-label">Profile</span>
          <Select
            value={pick}
            options={options}
            width="100%"
            onChange={(name) => {
              chosen.current = true;
              choose(name);
            }}
          />
        </label>
      )}
      {shown && lines.hint && <p className="shell-form-hint">{lines.hint}</p>}
      {lines.warn && <p className="shell-form-warn">{lines.warn}</p>}
    </ConnectionForm>
  );
}
