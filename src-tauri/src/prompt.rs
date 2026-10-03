//! The prompt editor's commands (section 6 of the build spec): the active
//! profile's `[prompt]` table, the designs other profiles hold, what a
//! capture compiles to, the candidates ring and the capture check, renders
//! with live or sample values and preview overrides, the preview the card
//! shows on your prompt, the edits the card makes, and the state the card
//! watches.
//!
//! Every command reads or writes the live profile under its lock and lets
//! go before it emits anything. A change to the table repaints the open
//! row through the session task, since only that task writes session
//! output, and every window hears `vosh://prompt-config-changed`.

pub(crate) mod last_seen;

use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio::time::Instant;
use vosh_prompt::capture::Recognizer;
use vosh_prompt::card::candidates::{CandidateGroup, CaptureCheck};
use vosh_prompt::card::describe::{Described, FormView};
use vosh_prompt::card::edit::EditOp;
use vosh_prompt::card::report::{CompileReport, CompileRequest};
use vosh_prompt::card::state::PromptState;
use vosh_prompt::config::PREVIOUS_TEMPLATES;
use vosh_prompt::values::overrides::{Overridden, Overrides, Preview, PromptPreview};
use vosh_prompt::values::Samples;
use vosh_prompt::{
    CaptureConfig, FieldRef, PromptConfig, RenderOptions, Rendered, Resolved, Template, Values,
};

use crate::app::events::broadcast_prompt_config_changed;
use crate::app::state::{SharedState, PROFILES_NOT_LOADED};
use crate::disk::save::{mark_profile_dirty, PERSIST_LOCK};
use crate::profile::Profile;

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
        request_repaint(state.inner()).await;
        broadcast_prompt_config_changed(&app);
    }
    Ok(())
}

/// The body of [`prompt_config_set`]: check and take the table. Returns
/// whether it changed anything. Only a capture that differs from the one
/// the profile holds is checked, since the game can hand the profile
/// codes that do not compile (`follow_game`), and the card sends that
/// capture back with every other change it saves.
pub(crate) fn set_config(p: &mut Profile, mut config: PromptConfig) -> Result<bool, String> {
    // Turning drawing on with no design draws Vosh's default, as Settings
    // and #prompt draw do.
    if config.draw && !p.prompt.config().draw && config.template.is_empty() {
        config.template = vosh_prompt::DEFAULT_DESIGN.to_string();
    }
    set_config_as_is(p, config)
}

/// [`set_config`] with the design exactly as sent, an empty one too, for
/// Start empty in the card.
pub(crate) fn set_config_as_is(p: &mut Profile, mut config: PromptConfig) -> Result<bool, String> {
    if config.capture != p.prompt.config().capture {
        vosh_prompt::card::report::check_capture(&config.capture, p.prompt.who())?;
    }
    config.previous_templates.truncate(PREVIOUS_TEMPLATES);
    let before = p.prompt.revision();
    p.set_prompt_config(config);
    Ok(p.prompt.revision() != before)
}

/// The card opened. When the design differs from the newest earlier one,
/// it goes first among the earlier designs, so trying a preset and
/// closing never loses it (section 5). Returns the table as it now
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

/// The body of [`prompt_card_open`]: the table as it now stands, and
/// whether opening changed it.
pub(crate) fn card_open(p: &mut Profile) -> (PromptConfig, bool) {
    let mut config = p.prompt.config().clone();
    if !config.note_opened() {
        return (config, false);
    }
    p.set_prompt_config(config.clone());
    (config, true)
}

/// Ask the session to repaint the open row as the table now says.
async fn request_repaint(state: &SharedState) {
    if let Some(handle) = state.session.lock().await.as_ref() {
        let _ = handle.prompt_repaint();
    }
}

/// A design another profile holds, for From another profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PromptDesign {
    pub profile: String,
    pub display_name: String,
    pub template: String,
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

/// The body of [`prompt_designs_list`]. Holds [`PERSIST_LOCK`] so a
/// switch cannot land between deciding which profile is live and reading
/// the others.
pub(crate) async fn designs(state: &SharedState) -> Result<Vec<PromptDesign>, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let guard = state.profile_set.lock().await;
    let set = guard.as_ref().ok_or(PROFILES_NOT_LOADED)?;
    let active = set.active_name().to_string();
    let mut out = Vec::new();
    for entry in set.list() {
        if entry.name == active {
            continue;
        }
        let Ok(config) = crate::characters::load_profile_file(set, &entry.name) else {
            continue;
        };
        let template = config.prompt_config().template;
        if template.is_empty() || template == vosh_prompt::DEFAULT_DESIGN {
            continue;
        }
        out.push(PromptDesign {
            display_name: crate::profile_set::display_name(&entry.name),
            profile: entry.name.clone(),
            template,
        });
    }
    Ok(out)
}

/// What a capture compiles to. It changes nothing.
#[tauri::command]
pub(crate) async fn prompt_compile(
    state: State<'_, SharedState>,
    capture: CompileRequest,
) -> Result<CompileReport, String> {
    Ok(compile(&*state.profile.lock().await, &capture))
}

/// The body of [`prompt_compile`]. Another game's presets read only the
/// values whose packages came this session.
pub(crate) fn compile(p: &Profile, request: &CompileRequest) -> CompileReport {
    vosh_prompt::card::report::report(request, p.prompt.who(), &|name| supplied(p, name))
}

/// True when GMCP supplied `name` this session: its package came.
fn supplied(p: &Profile, name: &str) -> bool {
    vosh_prompt::values::entry(name)
        .and_then(|e| e.package)
        .is_some_and(|package| p.prompt.vars.gmcp().has(package))
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

/// The body of [`prompt_capture_from_line`]. A ring entry that holds a
/// prompt of several lines gives its last, the one right before your
/// send, since Vosh reads another game's prompt from one line.
pub(crate) fn capture_from_line(
    p: &Profile,
    id: u64,
    names: &[String],
) -> Result<CompileReport, String> {
    let candidate = p
        .prompt
        .stage
        .candidate(id)
        .ok_or_else(|| "Vosh no longer keeps that line. Pick another one.".to_string())?;
    let line = candidate.plain.lines().last().unwrap_or_default();
    let mut report = vosh_prompt::card::report::line_report(line, names, &|name| supplied(p, name));
    report.gmcp_names = unknown_vitals(p);
    Ok(report)
}

/// The values in the latest Char.Vitals that Vosh has no name for, such
/// as `mp` on a game that calls mana that, for the card's name menu.
fn unknown_vitals(p: &Profile) -> Vec<vosh_prompt::card::report::GmcpName> {
    const PACKAGE: &str = "Char.Vitals";
    let Some(data) = p
        .prompt
        .vars
        .gmcp()
        .get(PACKAGE)
        .and_then(|d| d.as_object())
    else {
        return Vec::new();
    };
    data.iter()
        .filter(|(key, value)| {
            *key != "hidden" && value.is_number() && vosh_prompt::values::entry(key).is_none()
        })
        .map(|(key, _)| vosh_prompt::card::report::GmcpName {
            name: key.clone(),
            package: PACKAGE.to_string(),
        })
        .collect()
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

/// A Line trigger that matched your prompt as a line, for the card's row
/// after it saves a capture (D6). `pattern` is its first pattern, and
/// `preset` says a highlight preset installed it, which only the preset
/// changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LineTrigger {
    pub name: String,
    pub pattern: String,
    pub preset: bool,
}

/// The Line triggers that match a prompt `capture` reads in the
/// candidates ring, which no longer see it once the profile reads it
/// (D6). Each comes once, in the order they first match.
#[tauri::command]
pub(crate) async fn prompt_line_triggers(
    state: State<'_, SharedState>,
    capture: CaptureConfig,
) -> Result<Vec<LineTrigger>, String> {
    Ok(line_triggers(&*state.profile.lock().await, &capture))
}

/// The body of [`prompt_line_triggers`].
pub(crate) fn line_triggers(p: &Profile, capture: &CaptureConfig) -> Vec<LineTrigger> {
    let Some(recognizer) = Recognizer::compile_for(capture, p.prompt.who()) else {
        return Vec::new();
    };
    let mut out: Vec<LineTrigger> = Vec::new();
    for candidate in p.prompt.stage.ring() {
        let lines: Vec<&str> = candidate.plain.split('\n').collect();
        if recognizer
            .read(&lines)
            .or_else(|| recognizer.read_partial(&lines))
            .is_none()
        {
            continue;
        }
        for line in &lines {
            for trigger in vosh_automation::trigger::matching(
                &p.triggers,
                line,
                vosh_automation::trigger::MatchScope::Line,
            ) {
                if out.iter().any(|t| t.name == trigger.name) {
                    continue;
                }
                out.push(LineTrigger {
                    name: trigger.name.clone(),
                    pattern: trigger.first_pattern().to_string(),
                    preset: trigger.preset.is_some(),
                });
            }
        }
    }
    out
}

/// Which values a render draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ValuesFrom {
    /// The live values: the capture, scripts, GMCP and Vosh.
    #[default]
    Live,
    /// The catalog's samples, for a preview with no live data.
    Sample,
}

/// One render `prompt_render_many` draws.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub(crate) struct RenderRequest {
    pub template: String,
    #[serde(default)]
    pub values: ValuesFrom,
    /// One of the card's previews, Now, Low health, Fight or Lament.
    #[serde(default)]
    pub preview: Option<Preview>,
    /// Values on top of the preview's.
    #[serde(default)]
    pub overrides: Option<Overrides>,
    /// Draw each value with nothing to show as its label, as the open
    /// card does (D4).
    #[serde(default)]
    pub placeholders: bool,
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

/// The body of [`prompt_render`] and [`prompt_render_many`].
pub(crate) fn render_all(p: &Profile, requests: &[RenderRequest]) -> Vec<Rendered> {
    let vosh = crate::session::prompt_supplies(p, Instant::now());
    let live = p.prompt.vars.resolver(&vosh);
    let now = chrono::Local::now().naive_local();
    let samples = Samples { now };
    requests
        .iter()
        .map(|request| {
            let base: &dyn Values = match request.values {
                ValuesFrom::Live => &live,
                ValuesFrom::Sample => &samples,
            };
            let options = RenderOptions {
                placeholders: request.placeholders,
                ..RenderOptions::default()
            };
            let template = Template::parse(&request.template);
            let overrides = PromptPreview {
                preview: request.preview,
                overrides: request.overrides.clone(),
                ..PromptPreview::default()
            }
            .overrides(base);
            if overrides.is_empty() {
                vosh_prompt::render(&template, base, options)
            } else {
                vosh_prompt::render(&template, &Overridden::new(base, &overrides, now), options)
            }
        })
        .collect()
}

/// Draw with the live values, or a preview's on top of them, as the card
/// shows them.
fn with_values<T>(
    p: &Profile,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
    then: impl FnOnce(&dyn Values, bool) -> T,
) -> T {
    let vosh = crate::session::prompt_supplies(p, Instant::now());
    let live = p.prompt.vars.resolver(&vosh);
    let now = chrono::Local::now().naive_local();
    let over = PromptPreview {
        preview,
        overrides,
        ..PromptPreview::default()
    }
    .overrides(&live);
    if over.is_empty() {
        then(&live, false)
    } else {
        then(&Overridden::new(&live, &over, now), true)
    }
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

/// The body of [`prompt_describe`].
pub(crate) fn describe(
    p: &Profile,
    template: &str,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
) -> Described {
    let template = Template::parse(template);
    with_values(p, preview, overrides, |values, previewed| {
        vosh_prompt::card::describe::describe(&template, values, previewed)
    })
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

/// The body of [`prompt_forms`].
pub(crate) fn forms(p: &Profile, field: &str, preview: Option<Preview>) -> Vec<FormView> {
    let field = match field.split_once(':') {
        Some((name, param)) => FieldRef::with_param(name, param),
        None => FieldRef::new(field),
    };
    with_values(p, preview, None, |values, _| {
        vosh_prompt::card::describe::forms(&field, values)
    })
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
    request_repaint(state.inner()).await;
    Ok(())
}

/// The open card chose Aabahran's code reader on a host Vosh does not
/// know (More > Use Forsaken Lands prompt codes…), or let it go as it
/// closed. While it holds, the Forsaken Lands rules hold, so the game's
/// reply to `prompt` fills the card's fields on an older build (D17). It
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

/// A design after an edit, and how it draws with the live values and
/// placeholders, as the open card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Edited {
    pub template: String,
    pub rendered: Rendered,
    /// Where the piece the edit acted on sits now, so the card keeps it
    /// picked: the piece it changed or moved, or the one it added.
    pub piece: Option<usize>,
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

/// The body of [`prompt_edit`].
pub(crate) fn edit(p: &Profile, template: &str, op: &EditOp) -> Result<Edited, String> {
    let vosh = crate::session::prompt_supplies(p, Instant::now());
    let live = p.prompt.vars.resolver(&vosh);
    let known = |field: &FieldRef| !matches!(live.resolve(field), Resolved::Unknown);
    let (template, piece) =
        vosh_prompt::card::edit::apply_at(template, op, &known).map_err(|e| e.0)?;
    let options = RenderOptions {
        placeholders: true,
        ..RenderOptions::default()
    };
    let rendered = vosh_prompt::render_str(&template, &live, options);
    Ok(Edited {
        template,
        rendered,
        piece,
    })
}

/// The catalog with each field's live state and source, the status, the
/// new build sign and the open row with its spans.
#[tauri::command]
pub(crate) async fn prompt_state_get(state: State<'_, SharedState>) -> Result<PromptState, String> {
    let p = state.profile.lock().await;
    Ok(prompt_state(&p))
}

/// The body of [`prompt_state_get`], which `session://prompt-state`
/// carries too.
pub(crate) fn prompt_state(p: &Profile) -> PromptState {
    p.prompt
        .state(&crate::session::prompt_supplies(p, Instant::now()))
}

/// Watch your prompt: while on, `session://prompt-state` follows each
/// prompt Vosh reads.
#[tauri::command]
pub(crate) fn prompt_watch(state: State<'_, SharedState>, on: bool) {
    state.prompt_watch.store(on, Ordering::Release);
}

#[cfg(test)]
mod tests;
