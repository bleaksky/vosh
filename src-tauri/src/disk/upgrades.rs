//! The one time upgrades, each recorded under its id in `profiles.toml`,
//! and [`run`], the one ordered list of what launch upgrades before any
//! profile loads. The session records one more, `line_triggers.rs`, the
//! check of Line triggers against your prompt, at its end.

pub(crate) mod line_triggers;
pub(crate) mod presets;
pub(crate) mod prompt_capture;

use std::path::Path;

use tracing::{error, info};

use crate::disk::custom_themes::migrate_custom_themes;
use crate::disk::save::PERSIST_LOCK;
use crate::profile::set::ProfileSet;

/// Run the launch upgrades in order over `set`, the profile set launch
/// read from the app data folder `app_data`, before any profile loads,
/// and return the launch notices they leave. Each reads what the ones
/// before it wrote. `tests/upgrade_order.rs` pins the order.
///
/// 1. `prompt-capture-to-profile`, the move of the prompt capture
///    triggers into the profiles, see [`prompt_capture`].
/// 2. `preset-sent-tells-on`, then `preset-room-and-time-on`, the presets
///    a build adds, see [`presets::ROLLOUTS`].
/// 3. The move of the custom themes older profile files hold into
///    global.toml, see [`migrate_custom_themes`]. It records no id,
///    since after it runs no file holds a list for it to move.
///
/// The first two wait while `wizard_settled` is false, when a shared
/// catalog wizard run is still not done or waits for a relaunch, since a
/// profile file may still hold its items under their old group names.
/// They hold [`PERSIST_LOCK`] while they write, inactive profile files
/// too. The theme move runs whatever the run's state.
///
/// Two more steps run outside this list. [`ProfileSet::load_or_migrate`]
/// moves the root profile.toml of the oldest builds into
/// profiles/default.toml as launch reads the set, before any of these.
/// The session records `prompt-line-triggers` as it ends, see
/// [`line_triggers`].
pub(crate) async fn run(
    set: &mut ProfileSet,
    app_data: &Path,
    wizard_settled: bool,
) -> Vec<String> {
    let mut notices = Vec::new();
    if wizard_settled {
        let _persist = PERSIST_LOCK.lock().await;
        notices = prompt_capture::run(set, app_data);
        presets::run(set, app_data);
    }
    // global.toml owns the list from here on. The move writes only files
    // it read, so a file that does not read stays as it is.
    match migrate_custom_themes(set) {
        Ok(0) => {}
        Ok(files) => {
            info!(files, "moved custom themes into global.toml");
        }
        Err(e) => {
            error!(error = %e, "failed to move custom themes into global.toml");
        }
    }
    notices
}
