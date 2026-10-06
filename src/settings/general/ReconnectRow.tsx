import { useEffect, useState } from 'react';
import { reconnectGet, reconnectSet } from '../../ipc/session';
import { Row, Toggle } from '../../ui';
import { useShown } from '../shownProfile';

/** Whether the profile Settings shows dials again on its own when the
 *  link drops. It reads again when Settings moves to another profile,
 *  and a switch saves at once, back where it was when the save fails. */
export function ReconnectRow({ onError }: { onError: (message: string | null) => void }) {
  const profile = useShown().profile ?? undefined;
  const [on, setOn] = useState<boolean | null>(null);

  useEffect(() => {
    let cancelled = false;
    setOn(null);
    reconnectGet(profile)
      .then((next) => {
        if (!cancelled) setOn(next);
      })
      .catch((e) => {
        if (!cancelled) onError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [profile, onError]);

  const toggle = (next: boolean) => {
    setOn(next);
    reconnectSet(next, profile).then(
      () => onError(null),
      (e) => {
        setOn((now) => (now === next ? !next : now));
        onError(String(e));
      },
    );
  };

  return (
    <Row
      label="Reconnect when the link drops"
      description="Vosh dials up to 8 times over about 5 minutes, and you log in yourself. The game closes its login prompt after about 2 minutes."
      anchor="reconnect"
    >
      <Toggle checked={on ?? false} onChange={toggle} disabled={on === null} />
    </Row>
  );
}
