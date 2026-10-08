//! The commands of the writing card. The card hands a session's writer a
//! job, stops it, or takes the writer's offer to open on the text the
//! game just listed, and the writer answers on `session://writing`. The
//! card reads and keeps your drafts in writing.toml through the rest.

use tauri::State;

use crate::app::state::SharedState;
use crate::session::writer::payloads::WriteJob;
use crate::session::writer::WriterCommand;
use crate::sessions::SessionId;
use crate::writing::{self, Character, WritingFile};

/// What a command for the writer says when the session is not connected.
const NOT_CONNECTED: &str = "You're not connected, so Vosh can't reach the game.";

/// Hand the writer of `session`, or of the selected session, `command`.
async fn to_writer(
    state: &SharedState,
    session: Option<SessionId>,
    command: WriterCommand,
) -> Result<(), String> {
    let session = state.session(session)?;
    let slot = session.slot.lock().await;
    match slot.as_ref() {
        Some(handle) if handle.writer(command) => Ok(()),
        _ => Err(NOT_CONNECTED.to_string()),
    }
}

/// Start `job`: read a text, send one, post a note, send a text for its
/// review, clear the note the game holds, or pace a paste into the
/// editor you opened. It starts at the game's prompt, and how it ends
/// comes on `session://writing` with the job's id.
#[tauri::command]
pub(crate) async fn writing_start(
    state: State<'_, SharedState>,
    job: WriteJob,
    session: Option<SessionId>,
) -> Result<(), String> {
    to_writer(state.inner(), session, WriterCommand::Start(job)).await
}

/// Stop the job under way. Inside the game's editor the writer leaves it
/// with `@`, and a note it started goes with the board's `clear`.
#[tauri::command]
pub(crate) async fn writing_stop(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    to_writer(state.inner(), session, WriterCommand::Stop).await
}

/// Take the offer `id`: the writer leaves the game's editor with `@` and
/// reads what the game listed, or the note's fields, so the card opens on
/// them. An offer that went answers so.
#[tauri::command]
pub(crate) async fn writing_take(
    state: State<'_, SharedState>,
    id: u64,
    session: Option<SessionId>,
) -> Result<(), String> {
    to_writer(state.inner(), session, WriterCommand::Take { id }).await
}

/// The app data folder's writing.toml.
fn path(state: &SharedState) -> Result<std::path::PathBuf, String> {
    state
        .app_data
        .get()
        .map(|dir| crate::disk::paths::writing_path(dir))
        .ok_or_else(|| "Vosh has no app data folder yet.".to_string())
}

/// Every character's drafts and posts, with the card's switches.
#[tauri::command]
pub(crate) fn writing_file_get(state: State<'_, SharedState>) -> Result<WritingFile, String> {
    writing::read(&path(state.inner())?)
}

/// Keep `character`'s drafts and posts, in place of what the file held
/// for that character on its world.
#[tauri::command]
pub(crate) fn writing_character_set(
    state: State<'_, SharedState>,
    character: Character,
) -> Result<(), String> {
    writing::save_character(&path(state.inner())?, character)
}

/// Keep the card's Check spelling and whether its guide shows.
#[tauri::command]
pub(crate) fn writing_switches_set(
    state: State<'_, SharedState>,
    spelling: bool,
    guide: bool,
) -> Result<(), String> {
    writing::save_switches(&path(state.inner())?, spelling, guide)
}
