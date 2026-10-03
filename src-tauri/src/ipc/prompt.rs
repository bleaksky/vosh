//! The prompt editor's commands: the active profile's `[prompt]` table,
//! the designs other profiles hold, what a capture compiles to, the
//! candidates ring and the capture check, renders with live or sample
//! values and preview overrides, the preview the card shows on your
//! prompt, the edits the card makes, the state the card watches, and
//! where Vosh last saw your prompt settings. Every window reads where
//! your prompt shows, what the game hides and the triggers that hid your
//! prompt while the profile reads none.
//!
//! Every command reads or writes the live profile under its lock and lets
//! go before it emits anything. A change to the table repaints the open
//! row through the session task, since only that task writes session
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
    capture_from_line, card_open, compile, describe, designs, edit, forms, line_triggers,
    prompt_show_state, prompt_state, render_all, reported_hidden, request_prompt_repaint,
    set_config, set_config_as_is, Edited, LineTrigger, PromptDesign, PromptShowState,
    RenderRequest, ValuesFrom,
};

/// The active profile's `[prompt]` table.
#[tauri::command]
pub(crate) async fn prompt_config_get(
    state: State<'_, SharedState>,
) -> Result<PromptConfig, String> {
    Ok(state.profile.lock().await.prompt.config().clone())
}

/// Take a `[prompt]` table for the active profile. A new capture that does
/// not compile changes nothing, and the error says why in a sentence. A table
/// that changes anything saves shortly, repaints the open row and tells
/// every window. With `as_is`, the design is taken exactly as sent, so
/// Start empty keeps it empty as drawing turns on.
#[tauri::command]
pub(crate) async fn prompt_config_set<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    config: PromptConfig,
    as_is: Option<bool>,
) -> Result<(), String> {
    let p = &mut *state.profile.lock().await;
    let changed = if as_is.unwrap_or(false) {
        set_config_as_is(p, config)?
    } else {
        set_config(p, config)?
    };
    if changed {
        mark_profile_dirty(&app);
        request_prompt_repaint(state.inner()).await;
        broadcast_prompt_config_changed(&app);
    }
    Ok(())
}

/// The card opened. When the design differs from the newest earlier one,
/// it goes first among the earlier designs, so trying a preset and
/// closing never loses it. Returns the table as it now
/// stands. It saves shortly and tells every window when it changed.
#[tauri::command]
pub(crate) async fn prompt_card_open<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
) -> Result<PromptConfig, String> {
    let (config, changed) = card_open(&mut *state.profile.lock().await);
    if changed {
        mark_profile_dirty(&app);
        broadcast_prompt_config_changed(&app);
    }
    Ok(config)
}

/// The designs every other profile holds, read from their files, each
/// with a template that is not empty. These are designs you made, so a
/// template equal to Vosh's default design, [`vosh_prompt::DEFAULT_DESIGN`],
/// is left out, as a profile that never saved a file holds it and the
/// start list already offers it. A file holding a default an earlier
/// build shipped loads with today's, so it is left out too. A file Vosh
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
) -> Result<CompileReport, String> {
    Ok(compile(&*state.profile.lock().await, &capture))
}

/// What a capture built from one entry of the candidates ring reads: the
/// line another game prints before each command (P15). `names` names its
/// numbers in order, an empty name leaves one out, and the rest take the
/// names Vosh suggests from the letters after them. It changes nothing.
#[tauri::command]
pub(crate) async fn prompt_capture_from_line(
    state: State<'_, SharedState>,
    id: u64,
    names: Option<Vec<String>>,
) -> Result<CompileReport, String> {
    capture_from_line(&*state.profile.lock().await, id, &names.unwrap_or_default())
}

/// The candidates ring grouped by shape, with counts.
#[tauri::command]
pub(crate) async fn prompt_candidates(
    state: State<'_, SharedState>,
) -> Result<Vec<CandidateGroup>, String> {
    let p = state.profile.lock().await;
    Ok(vosh_prompt::card::candidates::groups(p.prompt.stage.ring()))
}

/// How a capture matches the candidates ring and the lines in your
/// scrollback.
#[tauri::command]
pub(crate) async fn prompt_capture_check(
    state: State<'_, SharedState>,
    capture: CaptureConfig,
) -> Result<CaptureCheck, String> {
    let (recognizer, ring) = {
        let p = state.profile.lock().await;
        let recognizer = Recognizer::compile_for(&capture, p.prompt.who());
        let ring: Vec<vosh_prompt::stage::Candidate> = p.prompt.stage.ring().cloned().collect();
        (recognizer, ring)
    };
    let lines: Vec<String> = {
        let scrollback = state.scrollback.lock().await;
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
) -> Result<Vec<LineTrigger>, String> {
    Ok(line_triggers(&*state.profile.lock().await, &capture))
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
) -> Result<Rendered, String> {
    let request = RenderRequest {
        template,
        values: values.unwrap_or_default(),
        preview,
        overrides,
        placeholders: placeholders.unwrap_or(false),
    };
    Ok(render_all(&*state.profile.lock().await, std::slice::from_ref(&request)).remove(0))
}

/// Draw several designs at once, such as the start list.
#[tauri::command]
pub(crate) async fn prompt_render_many(
    state: State<'_, SharedState>,
    requests: Vec<RenderRequest>,
) -> Result<Vec<Rendered>, String> {
    Ok(render_all(&*state.profile.lock().await, &requests))
}

/// What each piece and token of a design is, as the card shows it, with
/// what each value reads in the preview the card shows.
#[tauri::command]
pub(crate) async fn prompt_describe(
    state: State<'_, SharedState>,
    template: String,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
) -> Result<Described, String> {
    Ok(describe(
        &*state.profile.lock().await,
        &template,
        preview,
        overrides,
    ))
}

/// The forms a field takes, each drawn as the card shows it, for the
/// picker. `field` is written as a template names it, `hp` or
/// `aff:sanctuary`.
#[tauri::command]
pub(crate) async fn prompt_forms(
    state: State<'_, SharedState>,
    field: String,
    preview: Option<Preview>,
) -> Result<Vec<FormView>, String> {
    Ok(forms(&*state.profile.lock().await, &field, preview))
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
) -> Result<(), String> {
    state.profile.lock().await.prompt.set_preview(preview);
    request_prompt_repaint(state.inner()).await;
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
) -> Result<(), String> {
    state.profile.lock().await.prompt.set_reader(on);
    Ok(())
}

/// Apply one edit to a design. It writes nothing to the profile, so the
/// card saves the result with `prompt_config_set`.
#[tauri::command]
pub(crate) async fn prompt_edit(
    state: State<'_, SharedState>,
    template: String,
    op: EditOp,
) -> Result<Edited, String> {
    edit(&*state.profile.lock().await, &template, &op)
}

/// The catalog with each field's live state and source, the status, the
/// new build sign and the open row with its spans.
#[tauri::command]
pub(crate) async fn prompt_state_get(state: State<'_, SharedState>) -> Result<PromptState, String> {
    let p = state.profile.lock().await;
    Ok(prompt_state(&p))
}

/// Watch your prompt: while on, `session://prompt-state` follows each
/// prompt Vosh reads.
#[tauri::command]
pub(crate) fn prompt_watch(state: State<'_, SharedState>, on: bool) {
    state.prompt_watch.store(on, Ordering::Release);
}

/// Your prompt settings and where Vosh last saw them: the latest
/// Char.Prompt, else what the game showed after your own `prompt` this
/// session, else your log. None when none of them has one.
#[tauri::command]
pub(crate) async fn prompt_last_seen(
    state: State<'_, SharedState>,
) -> Result<Option<LastSeen>, String> {
    Ok(last_seen(state.inner()).await)
}

/// Which values the game hides, as every open window last heard it on
/// `session://hidden`. The session reports each change once, so a window
/// that opens or reloads while the game hides something, Settings among
/// them, reads the state here. Nothing is hidden with no connection.
#[tauri::command]
pub(crate) async fn hidden_get(
    state: State<'_, SharedState>,
) -> Result<vosh_prompt::values::Hidden, String> {
    Ok(reported_hidden(state.inner()).await)
}

/// Where the active profile's prompt shows, and whether it reads one.
/// Every window reads it again on `vosh://prompt-config-changed`.
#[tauri::command]
pub(crate) async fn prompt_show_get(
    state: State<'_, SharedState>,
) -> Result<PromptShowState, String> {
    Ok(prompt_show_state(&*state.profile.lock().await))
}

/// The triggers that hid your prompt this session while the profile
/// reads no prompt, so Vosh drew nothing in its place. The session names
/// each one once on `session://prompt-gag-without-reader`, so a window
/// that opens later, Settings among them, reads the list here. Empty
/// with no connection.
#[tauri::command]
pub(crate) async fn prompt_gags_without_reader(
    state: State<'_, SharedState>,
) -> Result<Vec<String>, String> {
    let p = state.profile.lock().await;
    Ok(p.prompt
        .stage
        .gags_without_reader()
        .map(str::to_string)
        .collect())
}
