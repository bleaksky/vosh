//! The commands for the command line. The words Vosh knows, so the page
//! colors what you type from what Vosh would run and never guesses, and
//! the history each session keeps across a restart.

use serde::Serialize;
use tauri::State;

use crate::app::state::{AppState, SharedState};
use crate::disk::paths::history_path;
use crate::input::history;
use crate::input::slash::SLASH_COMMANDS;
use crate::sessions::SessionId;

/// The first words Vosh acts on in a session, as the page reads them.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct KnownWords {
    /// The aliases that expand if you press Enter now, sorted.
    aliases: Vec<String>,
    /// The `#` commands Vosh runs, each without its `#`.
    commands: Vec<String>,
}

/// The aliases and `#` commands Vosh knows in `session`, or in the
/// selected session when it names none.
#[tauri::command]
pub(crate) async fn input_known_words(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<KnownWords, String> {
    known_words(&state, session).await
}

async fn known_words(state: &AppState, session: Option<SessionId>) -> Result<KnownWords, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(KnownWords {
        aliases: p.aliases.live_names(&c.plugin_aliases, c.stop_key),
        commands: SLASH_COMMANDS
            .iter()
            .map(|&name| name.to_string())
            .collect(),
    })
}

/// The commands `session`, or the selected session, sent in earlier
/// runs, oldest first.
#[tauri::command]
pub(crate) async fn history_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<String>, String> {
    let id = state.session(session)?.id;
    let Some(app_data) = state.app_data.get() else {
        return Ok(Vec::new());
    };
    Ok(history::load(&history_path(app_data, id)))
}

/// Keep `line`, a command `session` sent, for the next run. Answers false
/// for a line history never keeps, one that gives the game a password or
/// one you sent while logging in, see [`history::keeps`].
#[tauri::command]
pub(crate) async fn history_push(
    state: State<'_, SharedState>,
    line: String,
    session: Option<SessionId>,
) -> Result<bool, String> {
    push(&state, &line, session).await
}

async fn push(state: &AppState, line: &str, session: Option<SessionId>) -> Result<bool, String> {
    let session = state.session(session)?;
    let connected = session.live_address().is_some();
    let kept = {
        let p = session.lock_profile().await;
        let c = session.connection.lock();
        history::keeps(&p, &c, line, connected)
    };
    if !kept {
        return Ok(false);
    }
    let Some(app_data) = state.app_data.get() else {
        return Ok(true);
    };
    let id = session.id;
    // A session that closed while the line was on its way leaves no file.
    history::append(&history_path(app_data, id), line, || {
        state.session(Some(id)).is_ok()
    })
    .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use vosh_automation::alias::Alias;

    use super::{known_words, push, KnownWords};
    use crate::app::state::AppState;
    use crate::input::slash::SLASH_COMMANDS;
    use crate::profile::live::Profile;

    fn words(aliases: &[&str]) -> KnownWords {
        KnownWords {
            aliases: aliases.iter().map(|&a| a.to_string()).collect(),
            commands: SLASH_COMMANDS.iter().map(|&c| c.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn the_words_come_from_the_selected_session_or_the_named_one() {
        let state = AppState::default();
        let selected = state.selected_session();
        selected
            .lock_profile()
            .await
            .aliases
            .set(Alias::new("kk", "kick"));
        let mut orla = Profile::default();
        orla.aliases.set(Alias::new("hl", "cast heal"));
        let other = state.open_session(state.add_open_profile("Orla", orla));
        other
            .connection
            .lock()
            .plugin_aliases
            .set("mapper", "go", "run %1");

        assert_eq!(known_words(&state, None).await, Ok(words(&["kk"])));
        assert_eq!(
            known_words(&state, Some(other.id)).await,
            Ok(words(&["go", "hl"]))
        );
        let sent = serde_json::to_value(known_words(&state, None).await.unwrap()).unwrap();
        assert_eq!(sent["aliases"], serde_json::json!(["kk"]));
        assert_eq!(sent["commands"][0], "alias");
    }

    #[tokio::test]
    async fn a_pushed_command_lands_in_its_session_file_until_the_session_closes() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::default();
        state.app_data.set(dir.path().to_path_buf()).unwrap();
        let first = state.selected_session().id;
        let tolliver = state.open_session(state.add_open_profile("Tolliver", Profile::default()));
        let file = crate::disk::paths::history_path(dir.path(), tolliver.id);

        assert_eq!(push(&state, "look", Some(tolliver.id)).await, Ok(true));
        assert_eq!(
            push(&state, "look;password old new", Some(tolliver.id)).await,
            Ok(false)
        );
        assert_eq!(push(&state, "score", None).await, Ok(true));
        assert_eq!(crate::input::history::load(&file), ["look"]);
        assert_eq!(
            crate::input::history::load(&crate::disk::paths::history_path(dir.path(), first)),
            ["score"]
        );

        let id = tolliver.id;
        state.close_session(id).unwrap();
        assert!(push(&state, "where", Some(id)).await.is_err());
        assert_eq!(crate::input::history::load(&file), ["look"]);
    }
}
