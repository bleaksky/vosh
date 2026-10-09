//! The command for the words Vosh knows on the command line, so the page
//! colors what you type from what Vosh would run and never guesses.

use serde::Serialize;
use tauri::State;

use crate::app::state::{AppState, SharedState};
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

#[cfg(test)]
mod tests {
    use vosh_automation::alias::Alias;

    use super::{known_words, KnownWords};
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
}
