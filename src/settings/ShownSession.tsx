import { profileDisplayName } from '../lib/characterProfiles';
import { sessionLabel } from '../lib/sessionLabel';
import { listJoin } from '../lib/text';
import { useSessions } from '../stores/session/sessionsStore';
import { useShown } from './shownProfile';

// The session and the profile Settings edits, at the right of its
// header in the title button's order, by board 7 of the Sessions
// review: the dot, the session as its row reads, then the profile in
// the tertiary tone. While another session plays the same profile it
// adds Also in with that session, after a hairline, since an edit
// reaches both (board 9). It shows only while two or more sessions are
// open, so one session looks as before.

export function ShownSession() {
  const rows = useSessions();
  const { profile, session } = useShown();
  if (rows.length < 2 || profile === null) return null;
  const row = rows.find((r) => r.id === session) ?? null;
  const also = rows
    .filter((r) => r.id !== session && r.profile === profile)
    .map((r) => sessionLabel(r, rows).name);
  return (
    <span className="st-who">
      <span
        className={`shell-dot ${row?.connected ? 'is-connected' : 'is-idle'}`}
        aria-hidden="true"
      />
      {row && <span className="st-who-name">{sessionLabel(row, rows).name}</span>}
      <span className="st-who-profile">{profileDisplayName(profile)}</span>
      {also.length > 0 && <span className="st-who-also">Also in {listJoin(also)}</span>}
    </span>
  );
}
