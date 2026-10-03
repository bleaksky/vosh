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
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use vosh_prompt::config::{AabahranCapture, RegexCapture};

    use super::*;
    use crate::app::state::AppState;
    use crate::profile_config::ProfileConfig;
    use crate::profile_set::{ProfileSet, DEFAULT_PROFILE_NAME};

    fn codes(prompt: &str) -> CaptureConfig {
        CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            ..AabahranCapture::default()
        })
    }

    #[test]
    fn a_table_with_a_capture_that_does_not_compile_changes_nothing() {
        let mut p = Profile::default();
        let bad = PromptConfig {
            capture: codes("<`%h> "),
            ..PromptConfig::from_legacy(true, "%hp")
        };
        assert_eq!(
            set_config(&mut p, bad),
            Err("A color code runs into %h. Put a space between them in the game.".into())
        );
        assert!(p.prompt.config().is_default());
        let pattern = PromptConfig {
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![r"\[(?<hp>\d+".into()],
                ..RegexCapture::default()
            }),
            ..PromptConfig::default()
        };
        assert_eq!(
            set_config(&mut p, pattern),
            Err("Vosh cannot read that pattern.".into())
        );
    }

    #[test]
    fn codes_the_game_sent_that_do_not_compile_still_take_a_new_design() {
        let mut p = Profile::default();
        let follows = PromptConfig {
            capture: CaptureConfig::Aabahran(AabahranCapture {
                prompt: "<%hhp> ".into(),
                follow_game: true,
                ..AabahranCapture::default()
            }),
            ..PromptConfig::from_legacy(true, "%hp")
        };
        assert_eq!(set_config(&mut p, follows), Ok(true));
        // The game sends codes where a color runs into a code, and the
        // capture follows them as they are.
        let at = chrono::DateTime::parse_from_rfc3339("2026-09-30T12:00:00-05:00").unwrap();
        let _ = p.prompt.observe(
            "Char.Prompt",
            json!({"enabled": true, "prompt": "<`%hhp> ", "fprompt": ""}),
            at,
        );
        let CaptureConfig::Aabahran(sent) = &p.prompt.config().capture else {
            panic!("the capture reads codes");
        };
        assert_eq!(sent.prompt, "<`%hhp> ");
        assert!(!p.prompt.stage.has_recognizer());
        // The card saves a new design with the capture it read back.
        let edited = PromptConfig {
            template: "[%hp]".into(),
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, edited), Ok(true));
        assert_eq!(p.prompt.config().template, "[%hp]");
        // So does the switch.
        let off = PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, off), Ok(true));
        assert!(!p.prompt.config().draw);
        // Other codes that do not compile still change nothing.
        let other = PromptConfig {
            capture: codes("<`%mm> "),
            template: "[%mana]".into(),
            ..p.prompt.config().clone()
        };
        assert_eq!(
            set_config(&mut p, other),
            Err("A color code runs into %m. Put a space between them in the game.".into())
        );
        assert_eq!(p.prompt.config().template, "[%hp]");
    }

    #[test]
    fn a_table_that_compiles_is_taken_with_two_earlier_designs_at_most() {
        let mut p = Profile::default();
        let config = PromptConfig {
            previous_templates: vec!["a".into(), "b".into(), "c".into()],
            capture: codes("<%hhp %mm> "),
            ..PromptConfig::from_legacy(true, "%hp")
        };
        assert_eq!(set_config(&mut p, config.clone()), Ok(true));
        assert_eq!(p.prompt.config().previous_templates, ["a", "b"]);
        assert!(p.prompt.stage.has_recognizer());
        // A save writes the [ui] copy from the table, for older builds.
        let file = crate::profile_config::ProfileConfig::from_profile(&p);
        assert!(file.ui.prompt_template_enabled);
        assert_eq!(file.ui.prompt_template, "%hp");
        // The same table again changes nothing.
        assert_eq!(set_config(&mut p, config), Ok(false));
    }

    #[test]
    fn turning_drawing_on_with_no_design_takes_vosh_default() {
        let mut p = Profile::default();
        let config = PromptConfig {
            capture: codes("<%hhp> "),
            ..PromptConfig::from_legacy(false, "")
        };
        assert_eq!(set_config(&mut p, config), Ok(true));
        assert_eq!(p.prompt.config().template, "");
        let on = PromptConfig {
            draw: true,
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, on), Ok(true));
        assert_eq!(p.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
        // Start empty while drawing stays on keeps the design empty.
        let empty = PromptConfig {
            template: String::new(),
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, empty), Ok(true));
        assert_eq!(p.prompt.config().template, "");
    }

    #[test]
    fn start_empty_keeps_the_design_empty_as_drawing_turns_on() {
        // A fresh profile draws nothing and holds Vosh's default. Start
        // empty in the card's start list turns drawing on with no design,
        // and the design stays empty.
        let mut p = Profile::default();
        let fresh = PromptConfig {
            capture: codes("<%hhp> "),
            ..PromptConfig::fresh()
        };
        assert_eq!(set_config(&mut p, fresh), Ok(true));
        assert!(!p.prompt.config().draw);
        let empty = PromptConfig {
            template: String::new(),
            draw: true,
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config_as_is(&mut p, empty), Ok(true));
        assert!(p.prompt.config().draw);
        assert_eq!(p.prompt.config().template, "");
        // So does a design that was empty already.
        let off = PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, off), Ok(true));
        let again = PromptConfig {
            draw: true,
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config_as_is(&mut p, again), Ok(true));
        assert_eq!(p.prompt.config().template, "");
    }

    #[tokio::test]
    async fn designs_list_every_other_profile_with_a_design() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Second").unwrap();
        set.create("Third").unwrap();
        set.create("Fourth").unwrap();
        set.create("Fifth").unwrap();
        set.create("Sixth").unwrap();
        set.create("Seventh").unwrap();
        let mut second = ProfileConfig::default();
        second.set_prompt(PromptConfig::from_legacy(false, "[%hp]"));
        second.save(&set.profile_path("Second")).unwrap();
        // A design kept only in [ui], as older builds wrote it.
        let mut third = ProfileConfig::default();
        third.ui.prompt_template = "%mana".into();
        third.save(&set.profile_path("Third")).unwrap();
        // Fourth never saved a file, so it holds Vosh's default design,
        // which the start list already offers. Fifth saved that design.
        let mut fifth = ProfileConfig::default();
        fifth.set_prompt(PromptConfig::from_legacy(true, vosh_prompt::DEFAULT_DESIGN));
        fifth.save(&set.profile_path("Fifth")).unwrap();
        // Sixth changed the default design, so it holds a design of its
        // own.
        let sixth_design = vosh_prompt::DEFAULT_DESIGN.trim_end();
        let mut sixth = ProfileConfig::default();
        sixth.set_prompt(PromptConfig::from_legacy(true, sixth_design));
        sixth.save(&set.profile_path("Sixth")).unwrap();
        // Seventh saved the default an earlier build shipped, which
        // loads as today's.
        let mut seventh = ProfileConfig::default();
        seventh.set_prompt(PromptConfig::from_legacy(
            false,
            vosh_prompt::config::RETIRED_DEFAULTS[0],
        ));
        seventh.save(&set.profile_path("Seventh")).unwrap();
        let mut active = ProfileConfig::default();
        active.set_prompt(PromptConfig::from_legacy(true, "%move"));
        active
            .save(&set.profile_path(DEFAULT_PROFILE_NAME))
            .unwrap();
        let state: SharedState = Arc::new(AppState::default());
        *state.profile_set.lock().await = Some(set);
        let list = designs(&state).await.unwrap();
        let got: Vec<(&str, &str, &str)> = list
            .iter()
            .map(|d| {
                (
                    d.profile.as_str(),
                    d.display_name.as_str(),
                    d.template.as_str(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("Second", "Second", "[%hp]"),
                ("Third", "Third", "%mana"),
                ("Sixth", "Sixth", sixth_design),
            ]
        );
        let json = serde_json::to_value(&list[0]).unwrap();
        assert_eq!(
            json,
            json!({"profile": "Second", "display_name": "Second", "template": "[%hp]"})
        );

        // Without a profile set there is nothing to read.
        let empty: SharedState = Arc::new(AppState::default());
        assert_eq!(designs(&empty).await, Err(PROFILES_NOT_LOADED.to_string()));
    }

    #[test]
    fn compile_reports_with_the_values_this_session_supplies() {
        let mut p = Profile::default();
        let request: CompileRequest = serde_json::from_value(json!({
            "kind": "regex",
            "lines": [r"^<(?<hp>\d+)hp> $"],
        }))
        .unwrap();
        let ids = |report: &CompileReport| -> Vec<&'static str> {
            report.presets.iter().map(|preset| preset.id).collect()
        };
        assert_eq!(
            ids(&compile(&p, &request)),
            ["default", "minimal", "how_full", "detailed", "empty"]
        );
        p.prompt.connect(false);
        p.prompt.observe(
            "Char.Vitals",
            json!({"hp": 10, "maxhp": 20, "mana": 5, "maxmana": 9, "move": 1, "maxmove": 2}),
            chrono::Local::now().fixed_offset(),
        );
        assert_eq!(
            ids(&compile(&p, &request)),
            ["default", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
        );
        let codes: CompileRequest = serde_json::from_value(json!({
            "kind": "aabahran",
            "prompt": "[%h/%Hhp]",
            "typed": true,
        }))
        .unwrap();
        let report = compile(&p, &codes);
        assert!(report.ok);
        assert_eq!(report.prompt, "[%h/%Hhp] ");
    }

    #[test]
    fn a_ring_entry_becomes_a_capture_with_the_names_vosh_suggests() {
        let mut p = Profile::default();
        p.prompt.connect(false);
        let mut out = vosh_prompt::stage::Output::new(false);
        let line = "<100hp 50m 30mv> ";
        p.prompt
            .stage
            .line(&mut out, line.as_bytes(), line, None, b"");
        p.prompt.record(None, 1);
        let id = p.prompt.stage.ring().last().expect("the entry").id;
        let report = capture_from_line(&p, id, &[]).expect("the report");
        assert!(report.ok);
        // The names the numbers read into, without the ones you left out.
        let read_into = |report: &CompileReport| -> Vec<String> {
            report
                .numbers
                .iter()
                .filter(|n| !n.name.is_empty())
                .map(|n| n.name.clone())
                .collect()
        };
        assert_eq!(read_into(&report), ["hp", "mana", "move"]);
        assert_eq!(report.numbers.len(), 3);
        // Before Char.Vitals comes, the presets draw only what the line
        // reads.
        let ids = |report: &CompileReport| -> Vec<&'static str> {
            report.presets.iter().map(|preset| preset.id).collect()
        };
        let renamed =
            capture_from_line(&p, id, &["health".into(), String::new()]).expect("the report");
        assert_eq!(read_into(&renamed), ["health", "move"]);
        assert_eq!(
            ids(&renamed),
            ["default", "minimal", "how_full", "detailed", "empty"]
        );
        assert_eq!(
            capture_from_line(&p, id + 1, &[]),
            Err("Vosh no longer keeps that line. Pick another one.".into())
        );
        // Once Char.Vitals came, the presets draw every vital.
        p.prompt.observe(
            "Char.Vitals",
            json!({"hp": 10, "maxhp": 20, "mana": 5, "maxmana": 9, "move": 1, "maxmove": 2}),
            chrono::Local::now().fixed_offset(),
        );
        let report = capture_from_line(&p, id, &[]).expect("the report");
        assert_eq!(
            ids(&report),
            ["default", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
        );
        // The vitals this game sends that Vosh has no name for are offered
        // as names, each with its package.
        let leftover = &report.gmcp_names;
        assert!(leftover.is_empty(), "{leftover:?}");
        p.prompt.observe(
            "Char.Vitals",
            json!({"hp": 10, "maxhp": 20, "mp": 5, "mv": 9, "hidden": false}),
            chrono::Local::now().fixed_offset(),
        );
        let named = capture_from_line(&p, id, &[]).expect("the report");
        let names: Vec<(&str, &str)> = named
            .gmcp_names
            .iter()
            .map(|n| (n.name.as_str(), n.package.as_str()))
            .collect();
        assert_eq!(names, [("mp", "Char.Vitals"), ("mv", "Char.Vitals")]);
        // A prompt of several lines gives its last.
        let mut out = vosh_prompt::stage::Output::new(false);
        p.prompt.stage.line(&mut out, b"x", "x", None, b"");
        let block = "Tester: [===|---]\n<5hp> ";
        p.prompt.record(Some((block.as_bytes(), block)), 2);
        let id = p.prompt.stage.ring().last().expect("the entry").id;
        let report = capture_from_line(&p, id, &[]).expect("the report");
        assert_eq!(report.shapes[0].lines, [r"^<(?<hp>-?\d+)hp> +$"]);
        assert!(report.shapes[0].settle);
    }

    #[test]
    fn renders_draw_live_or_sample_values_with_overrides() {
        let mut p = Profile::default();
        p.prompt.connect(false);
        p.prompt.observe(
            "Char.Vitals",
            json!({"hp": 850, "maxhp": 900}),
            chrono::Local::now().fixed_offset(),
        );
        let requests: Vec<RenderRequest> = serde_json::from_value(json!([
            {"template": "%hp/%{maxhp}"},
            {"template": "%hp/%{maxhp}", "values": "sample"},
            {"template": "%hp/%{maxhp}", "overrides": {"values": {"hp": 180}}},
            {"template": "%hp/%{maxhp}", "overrides": {"lament": true}},
            {"template": "[%gold]", "placeholders": true},
            {"template": "%hp/%{maxhp}", "preview": "low_health"},
            {"template": "%hp", "preview": "low_health", "overrides": {"values": {"hp": 7}}},
            {"template": "%hp/%{maxhp}", "values": "sample", "preview": "lament"},
            {"template": "%hp", "preview": "now"},
        ]))
        .unwrap();
        let plain: Vec<String> = render_all(&p, &requests)
            .into_iter()
            .map(|r| r.plain)
            .collect();
        assert_eq!(
            plain,
            [
                "850/900",
                "1020/1020",
                "180/900",
                "?/?",
                "[Gold]",
                "180/900",
                "7",
                "?/?",
                "850"
            ]
        );
    }

    #[test]
    fn an_edit_writes_the_design_and_draws_it_with_placeholders() {
        let p = Profile::default();
        let op: EditOp = serde_json::from_value(json!({
            "op": "insert_field",
            "at": 1,
            "field": "gold",
        }))
        .unwrap();
        let edited = edit(&p, "[", &op).unwrap();
        assert_eq!(edited.template, "[%gold");
        assert_eq!(edited.rendered.plain, "[Gold");
        assert_eq!(edited.rendered.spans.len(), 2);
        assert_eq!(edited.piece, Some(1));
        let unknown: EditOp = serde_json::from_value(json!({
            "op": "insert_field",
            "at": 0,
            "field": "nope",
        }))
        .unwrap();
        assert_eq!(
            edit(&p, "[", &unknown),
            Err("Vosh does not know that value.".into())
        );
        // Each op reads from the card in its own shape.
        for op in [
            json!({"op": "set_format", "piece": 0, "format": {"format": "bar", "width": 6}}),
            json!({"op": "set_color", "piece": 0, "color": {"kind": "named", "index": 2}}),
            json!({"op": "set_style", "piece": 0, "style": "italic", "on": true}),
            json!({"op": "set_when", "piece": 0, "when": "not_fight"}),
            json!({"op": "set_text", "piece": 0, "text": "x"}),
            json!({"op": "remove", "piece": 0}),
            json!({"op": "insert_text", "at": 0, "text": "x"}),
            json!({"op": "insert_nl", "at": 0}),
            json!({"op": "move", "piece": 0, "to": 1}),
        ] {
            serde_json::from_value::<EditOp>(op.clone()).unwrap_or_else(|e| panic!("{op}: {e}"));
        }
    }

    #[test]
    fn a_design_is_described_with_the_values_the_card_shows() {
        let mut p = Profile::default();
        p.prompt.connect(false);
        p.prompt.observe(
            "Char.Vitals",
            json!({"hp": 850, "maxhp": 900}),
            chrono::Local::now().fixed_offset(),
        );
        let live = describe(&p, "[%hp]", None, None);
        let hp = &live.pieces[1];
        assert_eq!(hp.label, "Health");
        assert_eq!(hp.meta.as_deref(), Some("850 of 900"));
        let low = describe(&p, "[%hp]", Some(Preview::LowHealth), None);
        assert_eq!(
            low.pieces[1].meta.as_deref(),
            Some("180 of 900 in this preview")
        );
        assert_eq!(low.tokens.len(), 3);
        let json = serde_json::to_value(&live).unwrap();
        assert_eq!(json["pieces"][1]["format"], "value");
        assert_eq!(json["pieces"][1]["color"], json!({"kind": "default"}));
        assert_eq!(json["pieces"][1]["when"], "always");
        assert_eq!(json["tokens"][1]["kind"], "value");
        // The picker's forms, a field with a parameter among them.
        let hp_forms = forms(&p, "hp", None);
        assert_eq!(hp_forms[1].sample.plain, "850/900");
        assert_eq!(hp_forms[1].label, "Current and max");
        let json = serde_json::to_value(&hp_forms[0]).unwrap();
        assert_eq!(json["format"], "value");
        assert_eq!(json["segment"], "850");
        let labels: Vec<&str> = forms(&p, "aff:sanctuary", None)
            .iter()
            .map(|f| f.label)
            .collect();
        assert_eq!(labels, ["Time left", "Mark when on", "Mark when off"]);
    }

    fn line_trigger(
        name: &str,
        pattern: &str,
        target: vosh_automation::trigger::TriggerTarget,
    ) -> vosh_automation::trigger::Trigger {
        vosh_automation::trigger::Trigger {
            name: name.into(),
            patterns: vec![vosh_automation::trigger::TriggerPattern {
                pattern: pattern.into(),
                enabled: true,
            }],
            priority: 5,
            enabled: true,
            actions: Vec::new(),
            preset: None,
            group: None,
            target,
        }
    }

    #[test]
    fn line_triggers_that_match_a_prompt_the_capture_reads_are_named_once() {
        use vosh_automation::trigger::TriggerTarget::{Line, Prompt};
        let mut p = Profile::default();
        p.prompt.connect(true);
        for trigger in [
            line_trigger("Sleep when mana is low", r"\[\d+/\d+hp \d{1,2}/\d+mn", Line),
            line_trigger("Flee below 20 percent", r"\[(\d+)/(\d+)hp", Line),
            line_trigger("Already on prompts", r"hp", Prompt),
            line_trigger("Room exits", r"^\[Exits:", Line),
            vosh_automation::trigger::Trigger {
                enabled: false,
                ..line_trigger("Turned off", r"hp", Line)
            },
            vosh_automation::trigger::Trigger {
                preset: Some("vitals".into()),
                ..line_trigger("From a preset", r"mv\]", Line)
            },
        ] {
            p.triggers.set(trigger).unwrap();
        }
        let mut out = vosh_prompt::stage::Output::new(false);
        for (line, at) in [
            ("[Exits: south]", 1),
            ("[1020/1020hp 8/800mn 930/930mv] ", 2),
            ("[1020/1020hp 800/800mn 930/930mv] ", 3),
        ] {
            p.prompt
                .stage
                .line(&mut out, line.as_bytes(), line, None, b"");
            p.prompt.record(None, at);
        }
        let named = line_triggers(&p, &codes("[%h/%Hhp %m/%Mmn %v/%Vmv]"));
        let names: Vec<(&str, &str, bool)> = named
            .iter()
            .map(|t| (t.name.as_str(), t.pattern.as_str(), t.preset))
            .collect();
        assert_eq!(
            names,
            [
                (
                    "Sleep when mana is low",
                    r"\[\d+/\d+hp \d{1,2}/\d+mn",
                    false
                ),
                ("Flee below 20 percent", r"\[(\d+)/(\d+)hp", false),
                ("From a preset", r"mv\]", true),
            ]
        );
        // No capture reads nothing, so nothing is named.
        let leftover = &line_triggers(&p, &CaptureConfig::None);
        assert!(leftover.is_empty(), "{leftover:?}");
        let json = serde_json::to_value(&named[1]).unwrap();
        assert_eq!(
            json,
            json!({"name": "Flee below 20 percent", "pattern": r"\[(\d+)/(\d+)hp", "preset": false})
        );
    }

    #[test]
    fn the_state_lists_the_catalog_with_live_states() {
        let mut p = Profile::default();
        p.prompt.connect(true);
        p.prompt.observe(
            "Char.Prompt",
            json!({"enabled": true, "prompt": "%h ", "fprompt": ""}),
            chrono::Local::now().fixed_offset(),
        );
        let state = prompt_state(&p);
        assert!(state.new_build);
        // The Forsaken Lands rules hold on its host.
        assert!(state.forsaken);
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["status"]["status"], "no_capture");
        assert_eq!(json["open_row"], serde_json::Value::Null);
        let hp = json["catalog"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "hp")
            .unwrap();
        assert_eq!(hp["state"], "missing");
        assert_eq!(hp["group"], "vitals");
        assert_eq!(hp["label"], "Health");
    }

    #[test]
    fn opening_the_card_keeps_the_design_it_found_first() {
        let mut p = Profile::default();
        let config = PromptConfig {
            previous_templates: vec!["older".into()],
            ..PromptConfig::from_legacy(true, "%hp")
        };
        assert_eq!(set_config(&mut p, config), Ok(true));
        let (opened, changed) = card_open(&mut p);
        assert!(changed);
        assert_eq!(opened.previous_templates, ["%hp", "older"]);
        assert_eq!(p.prompt.config().previous_templates, ["%hp", "older"]);
        // Opening again on the same design changes nothing.
        let (again, changed) = card_open(&mut p);
        assert!(!changed);
        assert_eq!(again, opened);
        // An empty design is never kept.
        let empty = PromptConfig {
            template: String::new(),
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, empty), Ok(true));
        assert!(!card_open(&mut p).1);
        // A third design pushes the oldest out.
        let third = PromptConfig {
            template: "%move".into(),
            ..p.prompt.config().clone()
        };
        assert_eq!(set_config(&mut p, third), Ok(true));
        assert_eq!(card_open(&mut p).0.previous_templates, ["%move", "%hp"]);
    }
}
