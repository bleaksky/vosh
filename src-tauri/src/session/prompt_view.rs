//! What your prompt shows and what the webview hears of it: your design
//! drawn from the live values with what Vosh itself supplies, the custom
//! prompt's part of a connection, and the prompt vars, hidden state and
//! prompt state the session sends.

use std::collections::BTreeMap;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::warn;

use crate::app::events;
use crate::profile::live::Profile;
use crate::prompt::client_values;

use super::connection::Connection;

#[cfg(test)]
thread_local! {
    /// How many times this thread drew your design from the live values,
    /// so a test can tell what a step costs.
    pub(super) static RENDERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Your design drawn from the live values. The vosh-prompt resolver reads
/// the values the capture and scripts set, then the latest GMCP packets,
/// then what Vosh itself knows, and draws `?` for a value the game hides.
/// The spans say where each piece landed, which the open row keeps for
/// the prompt card.
fn render_prompt(p: &Profile, c: &Connection, now: Instant) -> vosh_prompt::Rendered {
    #[cfg(test)]
    RENDERS.with(|n| n.set(n.get() + 1));
    let client = client_values(p, c, now);
    vosh_prompt::render_str(
        &p.prompt.config().template,
        &p.prompt.vars.resolver(&client),
        p.prompt.render_options(false),
    )
}

/// What your prompt shows: the lines the game sent with drawing off, and
/// with it on, your design as the open card shows it, with the live
/// render behind it while that differs.
pub(super) struct PromptView {
    /// What your prompt shows, or None for the lines the game sent.
    shown: Option<vosh_prompt::Rendered>,
    /// The live render while `shown` is a preview in its place.
    live: Option<vosh_prompt::Rendered>,
}

impl PromptView {
    /// The view the stage takes, with where each piece of the design
    /// landed in what your prompt shows, which the open row keeps for the
    /// prompt card.
    pub(super) fn stage(&self) -> vosh_prompt::stage::View<'_> {
        vosh_prompt::stage::View {
            shown: self.shown.as_ref().map(|r| r.ansi.as_str()),
            live: self.live.as_ref().map(|r| r.ansi.as_str()),
            spans: self.shown.as_ref().map_or(&[], |r| r.spans.as_slice()),
            plain: self.shown.as_ref().map_or("", |r| r.plain.as_str()),
        }
    }
}

/// What your prompt shows now. With drawing on and the open card showing a
/// preview, the design draws with the preview's values, and with the labels
/// of values that have nothing to show while the card asks for them, or the
/// row shows the lines the game sent while the card reads your codes. The
/// live render rides behind it. Overrides never reach
/// `session://prompt-vars`, so the panes keep the live values.
pub(super) fn prompt_view(p: &Profile, c: &Connection, now: Instant) -> PromptView {
    if !p.prompt.draws() {
        return PromptView {
            shown: None,
            live: None,
        };
    }
    let live = render_prompt(p, c, now);
    let Some(preview) = p.prompt.preview() else {
        return PromptView {
            shown: Some(live),
            live: None,
        };
    };
    if preview.raw {
        return PromptView {
            shown: None,
            live: Some(live),
        };
    }
    let client = client_values(p, c, now);
    let resolver = p.prompt.vars.resolver(&client);
    let overrides = preview.overrides(&resolver);
    let shown = vosh_prompt::render_str(
        &p.prompt.config().template,
        &vosh_prompt::values::overrides::Overridden::new(
            &resolver,
            &overrides,
            chrono::Local::now().naive_local(),
        ),
        p.prompt.render_options(preview.placeholders),
    );
    PromptView {
        shown: Some(shown),
        live: Some(live),
    }
}

/// Start the custom prompt's session with no packets and no values.
/// `known_host` is whether the host is The Forsaken Lands, whose rules
/// also hold when the profile's capture reads Aabahran's codes.
pub(super) fn start_prompt(p: &mut Profile, known_host: bool) {
    p.prompt.connect(known_host);
    p.prompt.stage.set_collapse(p.ui.collapse_repeats);
}

/// The custom prompt's packets, values and hidden state go with the
/// connection. The webview stores clear on the disconnected state, so
/// the hidden state that ends here is never reported.
pub(super) fn end_prompt(p: &mut Profile) {
    p.prompt.disconnect();
    let _ = p.prompt.vars.take_hidden_change();
}

/// Keep a GMCP packet for the custom prompt, stamped with the local
/// time it arrived.
pub(super) fn observe_prompt_gmcp(p: &mut Profile, msg: &vosh_protocol::gmcp::Message) {
    p.prompt.observe(
        &msg.package,
        msg.data.clone(),
        chrono::Local::now().fixed_offset(),
    );
}

/// Tell the webview which values the game hides, when that changed
/// since the last report. The session calls it once per socket read and
/// after a send that starts a pulse.
pub(super) async fn emit_hidden_change<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
) {
    let change = profile.lock().await.prompt.vars.take_hidden_change();
    if let Some(hidden) = change {
        if let Err(e) = app.emit(events::HIDDEN, hidden) {
            warn!(error = %e, "failed to emit the hidden state");
        }
    }
}

/// Push the prompt vars to the frontend as a single snapshot, the fresh
/// values the capture and scripts set, with a value the game hides as
/// `?`. `always` sends them even when they read as the webview last heard
/// them, as a Lua action that set one asks. Otherwise they go only when
/// they changed, such as when a pulse left the capture's values stale.
/// The vitals store replaces its copy with the payload, so a value that
/// went stale or was unset drops out.
pub(super) async fn emit_prompt_vars<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    always: bool,
) {
    let vars = profile.lock().await.prompt.take_prompt_vars(always);
    if let Some(vars) = vars {
        send_prompt_vars(app, &vars);
    }
}

pub(super) fn send_prompt_vars<R: tauri::Runtime>(
    app: &AppHandle<R>,
    vars: &BTreeMap<String, String>,
) {
    if let Err(e) = app.emit(events::PROMPT_VARS, vars) {
        warn!(error = %e, "failed to emit prompt vars");
    }
}

/// The prompt state while the card watches your prompt, for
/// `session://prompt-state` after a repaint. None while it does not.
pub(super) fn watched_state<R: tauri::Runtime>(
    app: &AppHandle<R>,
    p: &Profile,
    c: &Connection,
) -> Option<vosh_prompt::card::state::PromptState> {
    watching_prompt(app).then(|| crate::prompt::prompt_state(p, c))
}

/// Send `state` on `session://prompt-state`, when there is one.
pub(super) fn emit_prompt_state<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: Option<vosh_prompt::card::state::PromptState>,
) {
    if let Some(state) = state {
        if let Err(e) = app.emit(events::PROMPT_STATE, state) {
            warn!(error = %e, "failed to emit the prompt state");
        }
    }
}

/// The prompt card watches your prompt (`prompt_watch`), so the prompt
/// state follows each prompt Vosh reads.
pub(super) fn watching_prompt<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<crate::app::state::SharedState>()
        .is_some_and(|state| {
            state
                .prompt_watch
                .load(std::sync::atomic::Ordering::Acquire)
        })
}
