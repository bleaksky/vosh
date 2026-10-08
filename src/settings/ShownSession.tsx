import { profileDisplayName } from '../lib/characterProfiles';
import { sessionLabel } from '../lib/sessionLabel';
import { listJoin } from '../lib/text';
import { useSelected, useSessions } from '../stores/session/sessionsStore';
import { useShown } from './shownProfile';

// The session and the profile Settings edits, at the right of its
// header in the title button's order, by board 7 of the Sessions
// review: the dot, the session as its row reads, then the profile in
// the tertiary tone. While another session plays the same profile it
// adds Also in with that session, after a hairline, since an edit
// reaches both (board 9). It shows only while two or more sessions are
// open, so one session looks as before.
//
// While a page holds its profile and the selected session plays
// another, the dot and a note turn to the warn tone, and the note names
// the session Settings follows once you save or discard.

export function ShownSession() {
  const rows = useSessions();
  const selected = useSelected();
  const { profile, session, held } = useShown();
  if ((rows.length < 2 && !held) || profile === null) return null;
  const label = (id: number | null) => {
    const row = rows.find((r) => r.id === id);
    return row ? sessionLabel(row, rows).name : null;
  };
  const row = rows.find((r) => r.id === session) ?? null;
  const name = label(session);
  const follows = held ? label(selected) : null;
  const also = held
    ? []
    : rows
        .filter((r) => r.id !== session && r.profile === profile)
        .map((r) => sessionLabel(r, rows).name);
  const tone = held ? 'is-warn' : row?.connected ? 'is-success' : 'is-off';
  return (
    <span className="st-who">
      <span className={`dot ${tone}`} aria-hidden="true" />
      {name && <span className="st-who-name">{name}</span>}
      <span className="st-who-profile">{profileDisplayName(profile)}</span>
      {also.length > 0 && <span className="st-who-also">Also in {listJoin(also)}</span>}
      {follows && <span className="st-who-note">Save or discard to follow {follows}</span>}
    </span>
  );
}
