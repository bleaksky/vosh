//! What the prompt editor's commands do, behind the thin wrappers in
//! [`ipc::prompt`](crate::ipc::prompt). It takes a `[prompt]` table for
//! the active profile, reads the designs other profiles hold, says what a
//! capture compiles to and which Line triggers it takes over, renders
//! with live or sample values and preview overrides, applies the edits
//! the card makes, and builds the state the card watches and where your
//! prompt shows. It also holds what Vosh itself supplies your prompt and
//! the report of what the game said of your prompt settings, which the
//! session and the profile switch both use.

pub(crate) mod last_seen;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::time::Instant;
use tracing::warn;
use vosh_prompt::capture::Recognizer;
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

use crate::app::events::{self, broadcast_list_changes, ListChanges};
use crate::app::state::SharedState;
use crate::disk::save::PERSIST_LOCK;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::session::connection::Connection;

/// The body of [`prompt_config_set`]: check and take the table. Returns
/// whether it changed anything. Only a capture that differs from the one
/// the profile holds is checked, since the game can hand the profile
/// codes that do not compile (`follow_game`), and the card sends that
/// capture back with every other change it saves.
///
/// [`prompt_config_set`]: crate::ipc::prompt::prompt_config_set
pub(crate) fn set_config(p: &mut Profile, mut config: PromptConfig) -> Result<bool, String> {
    // Turning drawing on with no design follows the game, as Settings,
    // the card's switch and #prompt draw do.
    if config.draw && !p.prompt.config().draw && config.template.is_empty() {
        config.mirror = true;
    }
    set_config_as_is(p, config)
}

/// [`set_config`] with the design exactly as sent, an empty one too, for
/// Start empty in the card. A table that says it follows the game takes
/// the design written from its codes in place of the one sent, so a
/// table read before the codes last changed never brings back the old
/// design.
pub(crate) fn set_config_as_is(p: &mut Profile, mut config: PromptConfig) -> Result<bool, String> {
    if config.capture != p.prompt.config().capture {
        vosh_prompt::card::report::check_capture(&config.capture, p.prompt.who())?;
    }
    config.previous_templates.truncate(PREVIOUS_TEMPLATES);
    let before = p.prompt.revision();
    p.set_prompt_config(config);
    Ok(p.prompt.revision() != before)
}

/// The body of [`prompt_card_open`]: the table as it now stands, and
/// whether opening changed it.
///
/// [`prompt_card_open`]: crate::ipc::prompt::prompt_card_open
pub(crate) fn card_open(p: &mut Profile) -> (PromptConfig, bool) {
    let mut config = p.prompt.config().clone();
    if !config.note_opened() {
        return (config, false);
    }
    p.set_prompt_config(config.clone());
    (config, true)
}

/// What decides how your prompt looks on screen: the switch, the design
/// and where it shows. A line that changes any of them repaints it.
pub(crate) fn prompt_look(
    p: &crate::profile::live::Profile,
) -> (bool, String, vosh_prompt::PromptShow) {
    let config = p.prompt.config();
    (config.draw, config.template.clone(), config.show)
}

/// Ask the session to repaint the open row as the `[prompt]` table now
/// says. Nothing happens with no connection, or when no drawn prompt is
/// the last thing on screen.
pub(crate) async fn request_prompt_repaint(state: &SharedState) {
    if let Some(handle) = state.session.lock().await.as_ref() {
        let _ = handle.prompt_repaint();
    }
}

/// What Vosh itself supplies to the custom prompt: the tick timer and
/// your target from the connection, the profile's name and the affects
/// you track. The clock reads the local time.
pub(crate) fn client_values(
    p: &Profile,
    c: &Connection,
    now: Instant,
) -> vosh_prompt::ClientValues {
    let tick = c
        .tick
        .remaining(&p.tick, now)
        .map(|left| vosh_prompt::values::Tick {
            remaining: i64::try_from(left.as_millis().div_ceil(1000)).unwrap_or(i64::MAX),
            interval: i64::try_from(p.tick.config.interval_secs).ok(),
            since: c
                .tick
                .elapsed(&p.tick, now)
                .and_then(|since| i64::try_from(since.as_secs()).ok()),
        });
    vosh_prompt::ClientValues {
        tick,
        target: c.target.name.clone(),
        profile: p.display_name.clone(),
        now: None,
        tracked: p
            .ui
            .tracked_affects
            .iter()
            .map(|t| t.name.clone())
            .collect(),
    }
}

/// Tell the webview what the game said of your prompt settings, on
/// `session://game-prompt-seen`. When the active profile's capture took
/// a new setting, the profile saves shortly and every window reads the
/// `[prompt]` table again.
pub(crate) fn report_game_prompt_seen<R: tauri::Runtime>(
    app: &AppHandle<R>,
    seen: Vec<vosh_prompt::GamePromptSeen>,
) {
    let applied = seen.iter().any(|s| s.applied);
    for payload in seen {
        if let Err(e) = app.emit(events::GAME_PROMPT_SEEN, payload) {
            warn!(error = %e, "failed to emit the game's prompt settings");
        }
    }
    if applied {
        crate::disk::save::mark_profile_dirty(app);
        broadcast_list_changes(app, ListChanges::PROMPT);
    }
}

/// A design another profile holds, for From another profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PromptDesign {
    pub profile: String,
    pub display_name: String,
    pub template: String,
}

/// The body of [`prompt_designs_list`]. Holds [`PERSIST_LOCK`] so a
/// switch cannot land between deciding which profile is live and reading
/// the others.
///
/// [`prompt_designs_list`]: crate::ipc::prompt::prompt_designs_list
pub(crate) async fn designs(state: &SharedState) -> Result<Vec<PromptDesign>, String> {
    let _persist_guard = PERSIST_LOCK.lock().await;
    let set = state.loaded_profile_set().await?;
    let active = set.active_name().to_string();
    let mut out = Vec::new();
    for stored in set.read_all() {
        if stored.name == active {
            continue;
        }
        let config = match stored.file {
            Some(Ok(file)) => file.config,
            Some(Err(e)) => {
                warn!(error = %e, path = %stored.path.display(), "profile file unreadable");
                continue;
            }
            None => ProfileConfig::fresh(),
        };
        let config = config.prompt_config();
        // A design that follows the game is that profile's codes, which
        // Same as the game offers for yours.
        let template = config.template;
        if config.mirror || template.is_empty() || template == vosh_prompt::DEFAULT_DESIGN {
            continue;
        }
        out.push(PromptDesign {
            display_name: crate::profile::set::display_name(stored.name),
            profile: stored.name.to_string(),
            template,
        });
    }
    Ok(out)
}

/// The body of [`prompt_compile`]. Another game's presets read only the
/// values whose packages came this session.
///
/// [`prompt_compile`]: crate::ipc::prompt::prompt_compile
pub(crate) fn compile(p: &Profile, request: &CompileRequest) -> CompileReport {
    vosh_prompt::card::report::report(request, p.prompt.who(), &|name| supplied(p, name))
}

/// True when GMCP supplied `name` this session: its package came.
fn supplied(p: &Profile, name: &str) -> bool {
    vosh_prompt::values::entry(name)
        .and_then(|e| e.package)
        .is_some_and(|package| p.prompt.vars.gmcp().has(package))
}

/// The body of [`prompt_capture_from_line`]. A ring entry that holds a
/// prompt of several lines gives its last, the one right before your
/// send, since Vosh reads another game's prompt from one line.
///
/// [`prompt_capture_from_line`]: crate::ipc::prompt::prompt_capture_from_line
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

/// A Line trigger that matched your prompt as a line, for the card's row
/// after it saves a capture, since a Line trigger no longer sees a prompt
/// the profile reads. `pattern` is its first pattern, and `preset` says a
/// highlight preset installed it, which only the preset changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LineTrigger {
    pub name: String,
    pub pattern: String,
    pub preset: bool,
}

/// The body of [`prompt_line_triggers`].
///
/// [`prompt_line_triggers`]: crate::ipc::prompt::prompt_line_triggers
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
    /// card does.
    #[serde(default)]
    pub placeholders: bool,
}

/// The body of [`prompt_render`] and [`prompt_render_many`].
///
/// [`prompt_render`]: crate::ipc::prompt::prompt_render
/// [`prompt_render_many`]: crate::ipc::prompt::prompt_render_many
pub(crate) fn render_all(p: &Profile, c: &Connection, requests: &[RenderRequest]) -> Vec<Rendered> {
    let client = client_values(p, c, Instant::now());
    let live = p.prompt.vars.resolver(&client);
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
    c: &Connection,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
    then: impl FnOnce(&dyn Values, bool) -> T,
) -> T {
    let client = client_values(p, c, Instant::now());
    let live = p.prompt.vars.resolver(&client);
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

/// The body of [`prompt_describe`].
///
/// [`prompt_describe`]: crate::ipc::prompt::prompt_describe
pub(crate) fn describe(
    p: &Profile,
    c: &Connection,
    template: &str,
    preview: Option<Preview>,
    overrides: Option<Overrides>,
) -> Described {
    let template = Template::parse(template);
    with_values(p, c, preview, overrides, |values, previewed| {
        vosh_prompt::card::describe::describe(&template, values, previewed)
    })
}

/// The body of [`prompt_forms`].
///
/// [`prompt_forms`]: crate::ipc::prompt::prompt_forms
pub(crate) fn forms(
    p: &Profile,
    c: &Connection,
    field: &str,
    preview: Option<Preview>,
) -> Vec<FormView> {
    let field = match field.split_once(':') {
        Some((name, param)) => FieldRef::with_param(name, param),
        None => FieldRef::new(field),
    };
    with_values(p, c, preview, None, |values, _| {
        vosh_prompt::card::describe::forms(&field, values)
    })
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

/// The body of [`prompt_edit`].
///
/// [`prompt_edit`]: crate::ipc::prompt::prompt_edit
pub(crate) fn edit(
    p: &Profile,
    c: &Connection,
    template: &str,
    op: &EditOp,
) -> Result<Edited, String> {
    let client = client_values(p, c, Instant::now());
    let live = p.prompt.vars.resolver(&client);
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

/// The body of [`prompt_state_get`], which `session://prompt-state`
/// carries too.
///
/// [`prompt_state_get`]: crate::ipc::prompt::prompt_state_get
pub(crate) fn prompt_state(p: &Profile, c: &Connection) -> PromptState {
    p.prompt.state(&client_values(p, c, Instant::now()))
}

/// The body of [`hidden_get`](crate::ipc::prompt::hidden_get).
pub(crate) async fn reported_hidden(state: &SharedState) -> vosh_prompt::values::Hidden {
    state.profile.lock().await.prompt.vars.reported()
}

/// Where your prompt shows, with what the Settings row and the main
/// window need beside it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PromptShowState {
    /// `text`, `lifted` or `pinned`, from `[prompt] show`.
    pub show: String,
    /// The profile has a capture that reads a prompt. Without one Vosh
    /// finds no prompt to lift or pin.
    pub capture: bool,
    /// Draw your prompt is on, for the palette's row.
    pub draw: bool,
    /// The game sent Char.Prompt this session.
    pub game_sent: bool,
    /// The rows the band above the command line keeps while your prompt
    /// shows pinned, the most any prompt the capture reads can take.
    pub zone: usize,
    /// You turned prompts off in the game.
    pub prompts_off: bool,
}

/// The body of [`prompt_show_get`](crate::ipc::prompt::prompt_show_get).
pub(crate) fn prompt_show_state(p: &crate::profile::live::Profile) -> PromptShowState {
    PromptShowState {
        show: p.prompt.show().name().to_string(),
        capture: p.prompt.stage.has_recognizer(),
        draw: p.prompt.config().draw,
        game_sent: p.prompt.vars.gmcp().prompt_seen(),
        zone: p.prompt.zone(),
        prompts_off: p.prompt.prompts_off(),
    }
}

#[cfg(test)]
pub(crate) mod tests;
