//! Who is logged in. The session sends it to every window after a
//! connect, a disconnect and the first sight of a character name after
//! login, and Settings > Characters reads it through
//! `session_identity_get`.

use serde::Serialize;
use tauri::AppHandle;

use crate::app::events::{broadcast, SESSION_IDENTITY_CHANGED};
use crate::app::state::SharedState;
use crate::sessions::Session;

/// Who is logged in, for the Characters group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SessionIdentity {
    pub host: String,
    pub port: u16,
    /// The character from Char.Status or Char.Name, once the MUD sends
    /// it.
    pub character: Option<String>,
    /// The live profile.
    pub profile: String,
    /// The profile whose login toggle claims `character` on this world.
    /// None when no profile claims it. See [`ProfileSet::claimed_by`].
    ///
    /// [`ProfileSet::claimed_by`]: crate::profile::set::ProfileSet::claimed_by
    pub claimed_by: Option<String>,
}

/// Who is logged in on `session`, or None while it runs no connection.
pub(crate) async fn session_identity(
    state: &SharedState,
    session: &Session,
) -> Option<SessionIdentity> {
    let connection = session
        .current_connection
        .lock()
        .ok()
        .and_then(|g| g.clone());
    let (host, port) = connection?;
    let character = session
        .current_character
        .lock()
        .ok()
        .and_then(|g| g.clone());
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref()?;
    let claimed_by = character
        .as_deref()
        .and_then(|c| set.claimed_by(&host, port, c));
    Some(SessionIdentity {
        profile: set.active_name().to_string(),
        host,
        port,
        character,
        claimed_by,
    })
}

pub(crate) async fn broadcast_session_identity<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Session,
) {
    let identity = session_identity(state, session).await;
    broadcast(app, SESSION_IDENTITY_CHANGED, &identity);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::state::AppState;
    use crate::profile::set::DEFAULT_PROFILE_NAME;
    use crate::profile::tests::james_like_set;

    #[tokio::test]
    async fn session_identity_reports_the_login_and_who_claims_it() {
        let dir = tempfile::tempdir().unwrap();
        let state: SharedState = Arc::new(AppState::default());
        *state.profile_set.lock().await = Some(james_like_set(dir.path()));
        let session = state.selected_session();
        assert_eq!(session_identity(&state, &session).await, None);

        *session.current_connection.lock().unwrap() =
            Some(("play.theforsakenlands.com".into(), 1848));
        let identity = session_identity(&state, &session).await.unwrap();
        assert_eq!(identity.character, None);
        assert_eq!(identity.claimed_by, None);
        assert_eq!(identity.profile, DEFAULT_PROFILE_NAME);

        *session.current_character.lock().unwrap() = Some("Ilsabet".into());
        let identity = session_identity(&state, &session).await.unwrap();
        assert_eq!(
            identity,
            SessionIdentity {
                host: "play.theforsakenlands.com".into(),
                port: 1848,
                character: Some("Ilsabet".into()),
                profile: DEFAULT_PROFILE_NAME.into(),
                claimed_by: Some(DEFAULT_PROFILE_NAME.into()),
            }
        );

        // A character no profile claims keeps the live profile and
        // reports no claim, so Characters can offer a new profile.
        *session.current_character.lock().unwrap() = Some("Ondrevar".into());
        let identity = session_identity(&state, &session).await.unwrap();
        assert_eq!(identity.claimed_by, None);
        assert_eq!(identity.profile, DEFAULT_PROFILE_NAME);
    }
}
