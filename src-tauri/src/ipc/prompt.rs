//! The prompt editor's commands: the active profile's `[prompt]` table,
//! the designs other profiles hold, what a capture compiles to, the
//! candidates ring and the capture check, renders with live or sample
//! values and preview overrides, the preview the card shows on your
//! prompt, the edits the card makes, the state the card watches, and
//! where Vosh last saw your prompt settings. Every window reads where
//! your prompt shows, what the game hides and the triggers that hid your
//! prompt while the profile reads none.
//!
//! Every command but the designs list acts on the session it names, or on
//! the selected session when it names none. Each reads or writes the live
//! profile and the prompt engine on the session's connection under their
//! locks, the profile's first, and lets go of the connection's before it
//! emits anything. A command that needs only the engine takes only the
//! connection's lock. A change to the table reaches the engine of every
//! other session on the profile too, which keeps what its own game showed
//! it. It repaints the open row of each session whose prompt it changed
//! through that session's task, since only that task writes session
//! output, and every window hears `vosh://prompt-config-changed`.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, State};
use vosh_prompt::capture::Recognizer;
use vosh_prompt::card::candidates::{CandidateGroup, CaptureCheck};
use vosh_prompt::card::describe::{Described, FormView};
use vosh_prompt::card::edit::EditOp;
use vosh_prompt::card::report::{CompileReport, CompileRequest};
use vosh_prompt::card::state::PromptState;
use vosh_prompt::values::overrides::{Overrides, Preview, PromptPreview};
use vosh_prompt::{CaptureConfig, PromptConfig, Rendered};

use crate::app::events::broadcast_prompt_config_changed;
use crate::app::state::SharedState;
use crate::disk::save::mark_profile_dirty;
use crate::prompt::last_seen::{last_seen, LastSeen};
use crate::prompt::{
    capture_from_line, card_open, choose_in_other_sessions, compile, describe, designs, edit,
    forms, line_triggers, prompt_show_state, prompt_state, render_all, reported_hidden,
    request_prompt_repaint, set_config, set_config_as_is, Edited, LineTrigger, PromptDesign,
    PromptShowState, RenderRequest, ValuesFrom,
};
use crate::sessions::SessionId;

/// The `[prompt]` table of the session's prompt engine: what you chose
/// on the profile it plays, with the codes and the design its own game
/// gave it.
#[tauri::command]
pub(crate) async fn prompt_config_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<PromptConfig, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    Ok(c.prompt.config().clone())
}

/// Take a `[prompt]` table for the active profile. A new capture that does
/// not compile changes nothing, and the error says why in a sentence. A table
/// that changes anything saves shortly, reaches every other session on the
/// profile, repaints the open row and tells every window. Turning drawing
/// on with an empty design follows the game. With `as_is`, the design is
/// taken exactly as sent, so Start empty keeps it empty as drawing turns
/// on.
#[tauri::command]
pub(crate) async fn prompt_config_set<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    config: PromptConfig,
    as_is: Option<bool>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    // The locks cover only the change. The repaint waits on the session
    // slot, which Disconnect holds while the loop ends, and the loop's end
    // takes the profile, so holding the profile here would hang both.
    let (open, chosen) = {
        let mut p = session.lock_profile().await;
        let c = &mut *session.connection.lock();
        let changed = if as_is.unwrap_or(false) {
            set_config_as_is(&mut p, c, config)?
        } else {
            set_config(&mut p, c, config)?
        };
        (p.open().clone(), changed.then(|| c.prompt.config().clone()))
    };
    if let Some(chosen) = chosen {
        mark_profile_dirty(&app, &open);
        choose_in_other_sessions(&state, session.id, &open, &chosen).await;
        request_prompt_repaint(&session).await;
        broadcast_prompt_config_changed(&app, &open);
    }
    Ok(())
}

/// The card opened. When the design differs from the newest earlier one,
/// it goes first among the earlier designs, so trying a preset and
/// closing never loses it. Returns the table as it now
/// stands. When it changed, it saves shortly, reaches every other session
/// on the profile and tells every window.
#[tauri::command]
pub(crate) async fn prompt_card_open<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<PromptConfig, String> {
    let session = state.session(session)?;
    let (open, (config, changed)) = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        (p.open().clone(), card_open(&mut p, &mut c))
    };
    if changed {
        mark_profile_dirty(&app, &open);
        choose_in_other_sessions(&state, session.id, &open, &config).await;
        broadcast_prompt_config_changed(&app, &open);
    }
    Ok(config)
}

/// The designs every other profile holds, read from their files, each
/// with a template that is not empty. These are designs you made, so a
/// design that follows the game is left out, and so is one equal to
/// Vosh's default design, [`vosh_prompt::DEFAULT_DESIGN`], which the
/// start list already offers. A file holding a default an earlier build
/// shipped loads following the game, so it is left out too. A file Vosh
/// cannot read is left out.
#[tauri::command]
pub(crate) async fn prompt_designs_list(
    state: State<'_, SharedState>,
) -> Result<Vec<PromptDesign>, String> {
    designs(state.inner()).await
}

/// What a capture compiles to. It changes nothing.
#[tauri::command]
pub(crate) async fn prompt_compile(
    state: State<'_, SharedState>,
    capture: CompileRequest,
    session: Option<SessionId>,
) -> Result<CompileReport, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    Ok(compile(&c, &capture))
}

/// What a capture built from one entry of the candidates ring reads: the
/// line another game prints before each command. `names` names its
/// numbers in order, an empty name leaves one out, and the rest take the
/// names Vosh suggests from the letters after them. It changes nothing.
#[tauri::command]
pub(crate) async fn prompt_capture_from_line(
    state: State<'_, SharedState>,
    id: u64,
    names: Option<Vec<String>>,
    session: Option<SessionId>,
) -> Result<CompileReport, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    capture_from_line(&c, id, &names.unwrap_or_default())
}

/// The candidates ring grouped by shape, with counts.
#[tauri::command]
pub(crate) async fn prompt_candidates(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<CandidateGroup>, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    Ok(vosh_prompt::card::candidates::groups(c.prompt.stage.ring()))
}

/// How a capture matches the candidates ring and the lines in your
/// scrollback.
#[tauri::command]
pub(crate) async fn prompt_capture_check(
    state: State<'_, SharedState>,
    capture: CaptureConfig,
    session: Option<SessionId>,
) -> Result<CaptureCheck, String> {
    let session = state.session(session)?;
    let (recognizer, ring) = {
        let c = session.connection.lock();
        let recognizer = Recognizer::compile_for(&capture, c.prompt.who());
        let ring: Vec<vosh_prompt::stage::Candidate> = c.prompt.stage.ring().cloned().collect();
        (recognizer, ring)
    };
    let lines: Vec<String> = {
        let scrollback = session.scrollback.lock().await;
        scrollback
            .lines()
            .map(vosh_protocol::ansi::plain_text)
            .collect()
    };
    Ok(vosh_prompt::card::candidates::check(
        recognizer.as_ref(),
        ring.iter(),
        lines.iter().map(String::as_str),
    ))
}

/// The Line triggers that match a prompt `capture` reads in the
/// candidates ring, which no longer see it once the profile reads it.
/// Each comes once, in the order they first match.
#[tauri::command]
pub(crate) async fn prompt_line_triggers(
    state: State<'_, SharedState>,
    capture: CaptureConfig,
    session: Option<SessionId>,
) -> Result<Vec<LineTrigger>, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(line_triggers(&p, &c, &capture))
}

/// Draw a design with live or sample values, a preview and overrides on
/// top.
#[tauri::command]
pub(crate) async fn prompt_render(
    state: State<'_, SharedState>,
    template: String,
    values: Option<ValuesFrom>,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
    placeholders: Option<bool>,
    session: Option<SessionId>,
) -> Result<Rendered, String> {
    let session = state.session(session)?;
    let request = RenderRequest {
        template,
        values: values.unwrap_or_default(),
        preview,
        overrides,
        placeholders: placeholders.unwrap_or(false),
        cols: None,
    };
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(render_all(&p, &c, std::slice::from_ref(&request)).remove(0))
}

/// Draw several designs at once, such as the start list.
#[tauri::command]
pub(crate) async fn prompt_render_many(
    state: State<'_, SharedState>,
    requests: Vec<RenderRequest>,
    session: Option<SessionId>,
) -> Result<Vec<Rendered>, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(render_all(&p, &c, &requests))
}

/// What each piece and token of a design is, as the card shows it, with
/// what each value reads in the preview the card shows.
#[tauri::command]
pub(crate) async fn prompt_describe(
    state: State<'_, SharedState>,
    template: String,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
    session: Option<SessionId>,
) -> Result<Described, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(describe(&p, &c, &template, preview, overrides))
}

/// The forms a field takes, each drawn as the card shows it, for the
/// picker. `field` is written as a template names it, `hp` or
/// `aff:sanctuary`.
#[tauri::command]
pub(crate) async fn prompt_forms(
    state: State<'_, SharedState>,
    field: String,
    preview: Option<Preview>,
    session: Option<SessionId>,
) -> Result<Vec<FormView>, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(forms(&p, &c, &field, preview))
}

/// Show what the open card shows on your prompt in place of the live
/// render, or the live render again with null: one of its previews,
/// values on top, the labels of values with nothing to show, or the line
/// the game sent while the card reads your codes. It repaints the open
/// row, which carries the live render as its restore, so only live
/// renders reach history. Nothing saves or goes to the game, and the
/// panes keep the live values. It lasts until the card clears it, the
/// main window loads again, or the connection goes.
#[tauri::command]
pub(crate) async fn prompt_preview_set(
    state: State<'_, SharedState>,
    preview: Option<PromptPreview>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    session.connection.lock().prompt.set_preview(preview);
    request_prompt_repaint(&session).await;
    Ok(())
}

/// The open card chose Aabahran's code reader on a host Vosh does not
/// know (More > Use Forsaken Lands prompt codes…), or let it go as it
/// closed. While it holds, the Forsaken Lands rules hold, so the game's
/// reply to `prompt` fills the card's fields on an older build. It
/// lasts until the card lets it go, the main window loads again, or
/// another profile takes over.
#[tauri::command]
pub(crate) async fn prompt_code_reader_set(
    state: State<'_, SharedState>,
    on: bool,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    session.connection.lock().prompt.set_reader(on);
    Ok(())
}

/// Apply one edit to a design. It writes nothing to the profile, so the
/// card saves the result with `prompt_config_set`.
#[tauri::command]
pub(crate) async fn prompt_edit(
    state: State<'_, SharedState>,
    template: String,
    op: EditOp,
    session: Option<SessionId>,
) -> Result<Edited, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    edit(&p, &c, &template, &op)
}

/// The catalog with each field's live state and source, the status, the
/// new build sign and the open row with its spans.
#[tauri::command]
pub(crate) async fn prompt_state_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<PromptState, String> {
    let session = state.session(session)?;
    let p = session.lock_profile().await;
    let c = session.connection.lock();
    Ok(prompt_state(&p, &c))
}

/// Watch your prompt: while on, `session://prompt-state` follows each
/// prompt Vosh reads.
#[tauri::command]
pub(crate) fn prompt_watch(
    state: State<'_, SharedState>,
    on: bool,
    session: Option<SessionId>,
) -> Result<(), String> {
    state
        .session(session)?
        .prompt_watch
        .store(on, Ordering::Release);
    Ok(())
}

/// Your prompt settings and where Vosh last saw them: the latest
/// Char.Prompt, else what the game showed after your own `prompt` this
/// session, else your log. None when none of them has one.
#[tauri::command]
pub(crate) async fn prompt_last_seen(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Option<LastSeen>, String> {
    let session = state.session(session)?;
    Ok(last_seen(state.inner(), &session).await)
}

/// Which values the game hides, as every open window last heard it on
/// `session://hidden`. The session reports each change once, so a window
/// that opens or reloads while the game hides something, Settings among
/// them, reads the state here. Nothing is hidden with no connection.
#[tauri::command]
pub(crate) async fn hidden_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<vosh_prompt::values::Hidden, String> {
    let session = state.session(session)?;
    Ok(reported_hidden(&session).await)
}

/// Where the active profile's prompt shows, and whether it reads one.
/// Every window reads it again on `vosh://prompt-config-changed`.
#[tauri::command]
pub(crate) async fn prompt_show_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<PromptShowState, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    Ok(prompt_show_state(&c))
}

/// The triggers that hid your prompt this session while the profile
/// reads no prompt, so Vosh drew nothing in its place. The session names
/// each one once on `session://prompt-gag-without-reader`, so a window
/// that opens later, Settings among them, reads the list here. Empty
/// with no connection.
#[tauri::command]
pub(crate) async fn prompt_gags_without_reader(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Vec<String>, String> {
    let session = state.session(session)?;
    let c = session.connection.lock();
    Ok(c.prompt
        .stage
        .gags_without_reader()
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    use crate::app::state::{AppState, SharedState};

    #[tokio::test]
    async fn a_table_save_lets_go_of_the_profile_while_disconnect_holds_the_session() {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let state: SharedState = app.state::<SharedState>().inner().clone();
        let session = state.selected_session();
        let mut config = state.selected_profile().await.prompt.clone();
        config.template = "<%hp>%mana".into();

        // Disconnect holds the session slot while the loop ends, and the
        // end of the loop takes the profile.
        let slot = session.slot.lock().await;
        let save = tokio::spawn({
            let app = app.handle().clone();
            let config = config.clone();
            async move {
                super::prompt_config_set(
                    app.clone(),
                    app.state::<SharedState>(),
                    config,
                    None,
                    None,
                )
                .await
            }
        });

        let took = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if state.selected_profile().await.prompt.template == config.template {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(
            took.is_ok(),
            "the profile stays held while the save waits to repaint"
        );
        assert!(!save.is_finished(), "the save waits for the session slot");

        drop(slot);
        save.await.unwrap().expect("the table saves");
    }
}
