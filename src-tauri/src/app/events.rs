//! Every event the app sends the page. Each name is a constant here,
//! with the payload it carries and the page function that hears it, and
//! [`broadcast`] sends an event to every window.
//!
//! Each event of the session's stream whose payload is an object names
//! the session that sent it in a `session` field beside the payload's
//! own, through [`crate::sessions::Session::emit`], and `session://output`
//! names it in [`crate::output::OutputPayload`]. The GMCP packages, each
//! the packet as the game sent it, `session://prompt-vars`, a map of
//! values, and `vosh://affect-full-changed`, a map of fulls, carry
//! `{session, data}` through [`crate::sessions::Session::emit_data`].
//!
//! Two names stay where they are built. The session sends each GMCP
//! package from a `format!` template in session/gmcp.rs,
//! `session://gmcp/` and the package name, because the contract test
//! reads that template as a family of names. `test://frame` is a test
//! seam in output.rs that the page never hears.
//!
//! The list events tell every window when the trigger or alias list
//! changes, so an open Settings page follows an edit made anywhere
//! else: `#trigger`, `#alias`, `#untrigger`, `#unalias`, `#endrec`,
//! `#import-tintin`, `#profile load`, a Lua `mud.alias` or
//! `mud.unalias`, an import, or a preset. Each store keeps a revision
//! that moves with its list. A step reads [`ListRevisions`] under the
//! profile lock before it runs and again after, and
//! [`broadcast_list_changes`] sends one event for each list that moved.
//!
//! The profile's `[prompt]` table rides along the same way, so a
//! `#prompt` or `#unprompt` line tells Settings to read the prompt
//! switch and design again. Settings saves its whole `[prompt]` table,
//! and a copy it read before would otherwise put the old ones back.
//!
//! So do the macro groups. The command line keeps its own map of the
//! macro keys that fire, so a `#group` line or a Lua
//! `mud.set_group_enabled` that turned a macro group on or off tells it
//! to read the groups again, or the keys of a group that is off go on
//! firing. A group of any list that turned on or off tells Settings too,
//! which shows a switch on each group heading.
//!
//! A replace (a profile switch, an import, `#profile load` or `reset`)
//! hands every window the profile's UI settings at once, so none keeps
//! a copy of the old profile's. [`profile_ui_events`] reads them in the
//! order they go out, and [`line_effect_events`] decides what a run of
//! typed lines sends.
//!
//! The windows show one profile, the profile in front, which is the one
//! the selected session plays. An event that carries one profile's
//! settings or lists goes out only while that profile is in front, see
//! [`AppState::in_front`], whichever session on it made the change: the
//! list events, the macros and the timers, the `[ui]` events, a replace
//! and `vosh://profile-switched`. A session on a profile behind tells the
//! windows only through its row on `vosh://sessions-changed`, and a
//! selection that brings its profile to the front sends that profile's
//! settings, so the windows lose nothing. The selected session alone
//! sends `vosh://session-identity-changed`, and a selection sends it
//! again.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use crate::app::state::{AppState, SharedState};
use crate::profile::live::Profile;
use crate::profile::open::OpenProfile;
use crate::profile::panes::PaneLayoutPersist;
use crate::session::connection::Connection;
use crate::tick::TickConfig;

/// Send `event` to every open window, once. One emit reaches every
/// listener in every window, main and Settings alike, whichever handle
/// it goes out through. This used to emit once through each open window,
/// so each listener heard the event once per open window. A page listens
/// through `listen`, which hears an event sent to any target, so
/// `emit_to` with a window label still reaches the page listeners in the
/// other windows.
pub(crate) fn broadcast<R: tauri::Runtime, S: serde::Serialize + ?Sized>(
    app: &AppHandle<R>,
    event: &str,
    payload: &S,
) {
    if let Err(e) = app.emit(event, payload) {
        warn!(error = %e, event, "broadcast failed");
    }
}

// The session's stream.

/// Text for the terminal, from the game, an echo or a repaint, in the
/// order the renderers take it. The payload is an
/// [`crate::output::OutputPayload`]. `onOutput` hears it.
pub(crate) const OUTPUT: &str = "session://output";
/// The connection is connecting, up or down. The payload is a
/// [`crate::session::StatePayload`]. `onState` hears it.
pub(crate) const STATE: &str = "session://state";
/// The game turned its echo off or on, so the command line hides what
/// you type or shows it again. The payload is an
/// [`crate::session::InputModePayload`]. `onInputMode` hears it.
pub(crate) const INPUT_MODE: &str = "session://input-mode";
/// Your target or the quick keys changed. The payload is a
/// [`crate::session::TargetPayload`]. `onTarget` hears it.
pub(crate) const TARGET: &str = "session://target";
/// The tick timer, four times a second and on each tick the game
/// sends. The payload is a [`crate::tick::TickPayload`]. `onTick` hears
/// it.
pub(crate) const TICK: &str = "session://tick";
/// A line a trigger routes to a pane, once for each pane. The payload
/// is a [`crate::session::RoutedPayload`]. `onRouted` hears it.
pub(crate) const ROUTED: &str = "session://routed";
/// The values the game hides changed. The payload is a
/// [`vosh_prompt::values::Hidden`]. `onHidden` hears it.
pub(crate) const HIDDEN: &str = "session://hidden";
/// Your prompt values, when they changed or a prompt Vosh read sends
/// them anyway. The payload is `{session, data}`, and `data` maps each
/// value's name to its text, with `?` for a value the game hides.
/// `onPromptVars` hears it.
pub(crate) const PROMPT_VARS: &str = "session://prompt-vars";
/// Whether Vosh reads your prompt, when that changed. The payload is
/// `{status, last_match_at}`. `onPromptStatus` and
/// `subscribePromptShowChanges` hear it.
pub(crate) const PROMPT_STATUS: &str = "session://prompt-status";
/// What the prompt card shows, after each prompt Vosh reads while the
/// card watches your prompt. The payload is a
/// [`vosh_prompt::card::state::PromptState`]. `onPromptState` hears it.
pub(crate) const PROMPT_STATE: &str = "session://prompt-state";
/// The game told Vosh your prompt settings. The payload is a
/// [`vosh_prompt::GamePromptSeen`]. `onGamePromptSeen` hears it.
pub(crate) const GAME_PROMPT_SEEN: &str = "session://game-prompt-seen";
/// A trigger hid your prompt while this profile reads no prompt, so
/// Vosh drew nothing in its place. The payload is a
/// [`crate::session::GagWithoutReaderPayload`].
/// `onPromptGagWithoutReader` hears it.
pub(crate) const PROMPT_GAG_WITHOUT_READER: &str = "session://prompt-gag-without-reader";
/// An alert rang in the session, from a trigger, a preset or Lua. The
/// payload is a [`crate::alert::AlertPayload`]. `onAlert` hears it.
/// The main window plays its tone, and the sessions sidebar marks the
/// row of a session you are not looking at. The page half of the alerts
/// will show its notice.
pub(crate) const ALERT: &str = "session://alert";
/// Something for you happened in the session, the event of an alert
/// preset or an alert a trigger or Lua raised, that rang nothing: its
/// alert is off or quiet, or the 10 second cap held it back. The payload
/// is the session alone. `onMark` hears it, and the sessions sidebar
/// marks the row as an alert that rang does (Sessions Q9). Vosh sends it
/// only for a session other than the selected one.
pub(crate) const MARK: &str = "session://mark";
/// The alerts of a Lua owner ended, as its plugin turned off, stopped or
/// loaded again. The payload is a [`crate::alert::AlertsEnded`]. No page
/// listener hears it yet.
pub(crate) const ALERTS_ENDED: &str = "session://alerts-ended";
/// Where the redial of the session stands after a drop: a wait, a try, a
/// failed try, the try that reached the game, the end of the tries, a
/// cancel, or why a drop does not redial. The payload is a
/// [`crate::session::reconnect::ReconnectPayload`]. `onReconnect` hears
/// it, and the session's row in the sessions sidebar shows the spinner
/// through the tries and the triangle when no link comes of them. The
/// reconnect notice will hear it too.
pub(crate) const RECONNECT: &str = "session://reconnect";
/// The `[lua]` lines a step added to the session's Output ring, and the
/// lines you typed in the Scripts console. The payload is a
/// [`crate::script::output::LuaOutputPayload`]. `subscribeLuaOutput`
/// hears it, and the Scripts page in Settings shows the lines in its
/// Console.
pub(crate) const LUA_OUTPUT: &str = "session://lua-output";
/// Your vitals text, drawn for the footer or the status line that
/// watches it through `vitals_text_watch`. The payload is a
/// [`vosh_prompt::vitals::VitalsText`], the rows at the live values and
/// at full values with which of them read a fight. `onVitalsText`
/// hears it.
pub(crate) const VITALS_TEXT: &str = "session://vitals-text";

// The lists.

/// Sent to every window when the trigger list changed. The payload is an
/// empty string. `subscribeTriggersChanged` hears it.
pub(crate) const TRIGGERS_CHANGED: &str = "vosh://triggers-changed";
/// Sent to every window when the alias list changed. The payload is an
/// empty string. `subscribeAliasesChanged` hears it.
pub(crate) const ALIASES_CHANGED: &str = "vosh://aliases-changed";
/// Sent to every window when the active profile's `[prompt]` table
/// changed. The payload names the profile, `{profile}`, see
/// [`PromptConfigChanged`]. `subscribePromptConfigChanged` hears it.
pub(crate) const PROMPT_CONFIG_CHANGED: &str = "vosh://prompt-config-changed";
/// Sent to every window when a macro group turned on or off: a `#group`
/// line, a Lua `mud.set_group_enabled`, or a loadout switch. The payload
/// is an empty string. `subscribeMacroGroupsChanged` hears it, and the
/// command line reads the groups again.
pub(crate) const MACRO_GROUPS_CHANGED: &str = "vosh://macro-groups-changed";
/// Sent to every window when a group of any list turned on or off: a
/// `#group` line, a Lua `mud.set_group_enabled`, or the switch on a group
/// heading in Settings. The payload is an empty string.
/// `subscribeGroupsChanged` hears it, and Settings reads the switches
/// again.
pub(crate) const GROUPS_CHANGED: &str = "vosh://groups-changed";
/// Sent to every window when a macro was set or removed, or an import
/// brought macros. The payload is the whole list of
/// [`crate::profile::live::Macro`]. `subscribeMacrosChanged` hears it.
pub(crate) const MACROS_CHANGED: &str = "vosh://macros-changed";
/// Sent to every window when a timer was set or removed. The payload is
/// the whole list of [`crate::profile::live::Timer`].
/// `subscribeTimersChanged` hears it.
pub(crate) const TIMERS_CHANGED: &str = "vosh://timers-changed";
/// Sent to every window when `preset_edits_set` saved your edits to a
/// preset. The payload names the profile, `{profile}`, see
/// [`PresetEditsChanged`]. In loadout mode every profile shares the edits.
pub(crate) const PRESET_EDITS_CHANGED: &str = "vosh://preset-edits-changed";

// Plugins.

/// Sent to every window when the plugins changed: the Scripts page made
/// one, saved one, turned one on or off in a profile or loaded one again,
/// a profile switch turned a session's plugins over, or Vosh stopped one
/// in a session. The payload is null. `subscribePluginsChanged` hears
/// it, and the Scripts page in Settings reads the list again.
pub(crate) const PLUGINS_CHANGED: &str = "vosh://plugins-changed";

// Profiles.

/// Sent to every window when a profile was made, renamed, duplicated,
/// deleted or imported, its login or world changed, or the sharing scope
/// changed.
/// The payload is the profile's name, or `"scope"` after a scope change.
/// `subscribeProfilesChanged` hears it.
pub(crate) const PROFILES_CHANGED: &str = "vosh://profiles-changed";
/// Sent to every window once the profile in front changed, after the
/// events that carry its UI config: the selected session switched
/// profiles, or the selection moved to a session on another profile. The
/// payload is that profile's name. `subscribeProfileSwitched` and the
/// pane layout's `ensureListening` hear it.
pub(crate) const PROFILE_SWITCHED: &str = "vosh://profile-switched";
/// Sent after an edit to one profile's detail, or an import to it,
/// active or not, naming it as `{ name }`. Unlike
/// `vosh://tracked-affects-changed` and `vosh://pane-layout-changed` it
/// carries no data, so an edit to an inactive profile can never reach the
/// main window's stores.
/// `subscribeProfileChanged` hears it.
pub(crate) const PROFILE_CHANGED: &str = "vosh://profile-changed";
/// Sent with the selected session's
/// [`crate::session::identity::SessionIdentity`], or null, after its
/// connect, its disconnect, and the first sight of a character name
/// after its login, and again as a selection brings a session to the
/// front. Settings is its own webview and may open after all of those,
/// so it also reads the current value with `session_identity_get`.
/// `subscribeSessionIdentity` hears it.
pub(crate) const SESSION_IDENTITY_CHANGED: &str = "vosh://session-identity-changed";
/// Sent to every window when Vosh selected a session itself, as a click
/// on an alert banner does. The payload is a
/// [`crate::alert::banner::SessionSelected`]. `onSessionSelected` hears
/// it, and the sessions store reads the list again.
pub(crate) const SESSION_SELECTED: &str = "vosh://session-selected";
/// Sent to every window with every session's row after a step that
/// changed what a row shows: a session opened, closed, moved, was renamed
/// or selected, or took a place to dial, one of its connects started,
/// connected or ended, a login named its character, or the profile it
/// plays switched or took a new name. The payload is the list of
/// [`crate::sessions::SessionRow`] in list order, with `selected` set on
/// one. `onSessionsChanged` hears it, and the sessions store takes the
/// rows.
pub(crate) const SESSIONS_CHANGED: &str = "vosh://sessions-changed";
/// Sent to every window when the game of a session turns to day or
/// night, from World.Time (Alerts Q16). The payload is a
/// [`crate::tick::DaylightPayload`] with the session beside it. Switch
/// themes With the game reads it in the page half. No page listener
/// hears it yet.
pub(crate) const DAYLIGHT_CHANGED: &str = "vosh://daylight-changed";
/// Sent to every window when sharing the theme category added to the
/// live custom themes. The payload is the whole list of
/// [`crate::profile::ui::CustomTheme`].
/// `subscribeCustomThemesChanged` hears it.
pub(crate) const CUSTOM_THEMES_CHANGED: &str = "vosh://custom-themes-changed";
/// Sent to every window when the active loadouts changed. The payload
/// is null. `subscribeLoadoutsChanged` hears it.
pub(crate) const LOADOUTS_CHANGED: &str = "vosh://loadouts-changed";
/// Sent to every window once the wizard wrote its files. The payload is
/// null. `subscribeMigrationApplied` hears it.
pub(crate) const MIGRATION_APPLIED: &str = "vosh://migration-applied";
/// Sent to every window with the whole map of a session's connection
/// whenever it changes. The payload is `{session, data}`, and `data` is
/// the [`crate::affects::full::FullMap`]. `subscribeAffectFullChanged`
/// hears it.
pub(crate) const AFFECT_FULL_CHANGED: &str = "vosh://affect-full-changed";

// The profile's `[ui]` table.

/// Sent last by [`broadcast_profile_ui`]. The live
/// profile's whole UI config was replaced, by a switch, an import,
/// `#profile load` or `reset`. Settings reads the new config here and
/// drops a save still waiting, which would write what you changed on
/// the old profile onto the new one. The payload is null.
/// `subscribeUiConfigReplaced` hears it.
pub(crate) const UI_CONFIG_REPLACED: &str = "vosh://ui-config-replaced";
/// Sent to every window with the pane layout whenever it changes: a
/// pane edit, a reset, or a replace. The payload is a
/// [`PaneLayoutEnvelope`]. The pane layout's
/// `ensureListening` hears it.
pub(crate) const PANE_LAYOUT_CHANGED: &str = "vosh://pane-layout-changed";
/// Sent to every window with the tracked affects whenever they change:
/// a Settings save or a replace. The payload is the list of
/// [`crate::profile::ui::TrackedAffect`].
/// `subscribeTrackedAffectsChanged` hears it.
pub(crate) const TRACKED_AFFECTS_CHANGED: &str = "vosh://tracked-affects-changed";
/// Sent to every window with which way the status line tick counts, on
/// a replace. Settings sends its own saves. The payload is the way,
/// such as `"up"`. `subscribeTickCountChanged` hears it.
pub(crate) const TICK_COUNT_CHANGED: &str = "vosh://tick-count-changed";
/// Sent to every window with the clock the status line reads the game
/// time on, on a replace. Settings sends its own saves. The payload is
/// the clock, `"24h"` or `"12h"`. `subscribeGameTimeChanged` hears it.
pub(crate) const GAME_TIME_CHANGED: &str = "vosh://game-time-changed";
/// Sent to every window with the style of the time and tick chips, on
/// a replace. Settings sends its own saves. The payload is the style,
/// such as `"value_only"`. `subscribeChipStyleChanged` hears it.
pub(crate) const CHIP_STYLE_CHANGED: &str = "vosh://chip-style-changed";
/// Sent to every window with the Affects pane's style, marker, tint,
/// and the hours at which an affect runs out and is almost gone,
/// whenever they change: a pick from the pane menu, a Settings save
/// (which the frontend sends itself), or a replace. The payload is an
/// [`AffectsDisplay`].
/// `subscribeAffectsDisplayChanged` hears it.
pub(crate) const AFFECTS_DISPLAY_CHANGED: &str = "vosh://affects-display-changed";
/// Sent to every window with the chat pane's channel colors whenever
/// they change: a pick or a reset from the pane menu, or a replace. The
/// payload maps each channel to its ANSI slot.
/// `subscribeChatColorsChanged` hears it.
pub(crate) const CHAT_COLORS_CHANGED: &str = "vosh://chat-colors-changed";
/// Sent to every window with the tick settings whenever they change:
/// a Settings Tick save, a `#tick` command, or a replace. The payload is
/// a [`crate::tick::TickConfig`].
/// `subscribeTickConfigChanged` hears it.
pub(crate) const TICK_CONFIG_CHANGED: &str = "vosh://tick-config-changed";

// Windows and the menu.

/// Sent to the main window on `#help <words>`. The payload is the
/// words. `useAppCommands` hears it and opens Help on the best match.
pub(crate) const HELP_OPEN: &str = "vosh://help-open";
/// Sent to every window on quit. The payload is the round number, which
/// each window's answer names. `listenForQuitFlush` hears it.
pub(crate) const FLUSH_PENDING_WRITES: &str = "vosh://flush-pending-writes";
/// Sent to the main window when you choose a menu command. The payload
/// is the command's id. `listenAppMenu` hears it.
#[cfg(target_os = "macos")]
pub(crate) const APP_MENU: &str = "vosh://app-menu";
/// Find, chosen while Settings is in front, focuses the settings search.
/// The payload is null. `Sidebar` hears it.
#[cfg(target_os = "macos")]
pub(crate) const SETTINGS_FIND: &str = "vosh://settings-find";
/// Find, chosen while Help is in front, focuses the help search. The
/// payload is null. `HelpWindow` hears it.
#[cfg(target_os = "macos")]
pub(crate) const HELP_FIND: &str = "vosh://help-find";

// The native renderer.

/// The native surface's grid size changed, so the hidden xterm takes
/// the same grid. The payload is `[cols, rows]`. `Terminal` hears it.
#[cfg(native_surface)]
pub(crate) const NATIVE_GRID_SIZE: &str = "vosh://native-grid-size";
/// How far back the native surface shows changed, or a selection showed
/// the grid of another session. The payload is `[offset, max]` in rows.
/// `startNativeScroll` hears it.
#[cfg(native_surface)]
pub(crate) const NATIVE_SCROLL: &str = "vosh://native-scroll";
/// The native surface copied your selection. The payload is the count
/// of characters copied. `startCopyToasts` hears it.
#[cfg(native_surface)]
pub(crate) const NATIVE_COPIED: &str = "vosh://native-copied";
/// A click the page forwarded to the native surface ended. The page
/// cancels the press, so no DOM mouseup follows, and the command line
/// takes focus from this instead. The payload is null.
/// `useNativeSurfaceBridge` hears it.
#[cfg(native_surface)]
pub(crate) const TERMINAL_CLICKED: &str = "vosh://terminal-clicked";
/// The pointer over the native surface wants another cursor. The
/// payload is the CSS cursor name. `useNativeSurfaceBridge` hears it.
#[cfg(native_surface)]
pub(crate) const TERMINAL_CURSOR: &str = "vosh://terminal-cursor";

/// The payload of [`PROMPT_CONFIG_CHANGED`]: the profile in front, whose
/// table changed, None before any profile loads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PromptConfigChanged {
    pub(crate) profile: Option<String>,
}

/// The payload of [`PRESET_EDITS_CHANGED`]: the profile whose edits
/// changed, None before any profile loads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PresetEditsChanged {
    pub(crate) profile: Option<String>,
}

/// Tell every window the `[prompt]` table of `open` changed, while it is
/// the profile in front.
pub(crate) fn broadcast_prompt_config_changed<R: tauri::Runtime>(
    app: &AppHandle<R>,
    open: &Arc<OpenProfile>,
) {
    if in_front(app, open) {
        let profile = open.name();
        broadcast(app, PROMPT_CONFIG_CHANGED, &PromptConfigChanged { profile });
    }
}

/// Whether `open` is the profile in front, see [`AppState::in_front`].
/// An app that holds no state plays no profile, so none is.
fn in_front<R: tauri::Runtime>(app: &AppHandle<R>, open: &Arc<OpenProfile>) -> bool {
    app.try_state::<SharedState>()
        .is_some_and(|state| state.in_front(open))
}

/// The trigger and alias list revisions, the prompt table's, and the
/// counts of macro group toggles and of group toggles in any list, at
/// one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListRevisions {
    triggers: u64,
    aliases: u64,
    prompt: u64,
    macro_groups: u64,
    groups: u64,
}

impl ListRevisions {
    /// The revisions of `profile`'s lists, and of the prompt table the
    /// engine on `c` holds, which the profile keeps a copy of.
    pub(crate) fn of(profile: &Profile, c: &Connection) -> Self {
        Self {
            prompt: c.prompt.revision(),
            ..Self::of_lists(profile)
        }
    }

    /// The revisions of `profile`'s lists alone, for a Settings step that
    /// never changes the prompt table. It reads no session's connection,
    /// so it serves a profile whichever sessions play it.
    pub(crate) fn of_lists(profile: &Profile) -> Self {
        Self {
            triggers: profile.triggers.revision(),
            aliases: profile.aliases.revision(),
            prompt: 0,
            macro_groups: profile.macro_group_toggles,
            groups: profile.group_toggles,
        }
    }
}

/// Which lists a step changed, whether it changed the prompt table,
/// whether it turned a macro group on or off, and whether it turned a
/// group of any list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ListChanges {
    pub(crate) triggers: bool,
    pub(crate) aliases: bool,
    pub(crate) prompt: bool,
    pub(crate) macro_groups: bool,
    pub(crate) groups: bool,
}

impl ListChanges {
    pub(crate) fn between(before: ListRevisions, after: ListRevisions) -> Self {
        Self {
            triggers: before.triggers != after.triggers,
            aliases: before.aliases != after.aliases,
            prompt: before.prompt != after.prompt,
            macro_groups: before.macro_groups != after.macro_groups,
            groups: before.groups != after.groups,
        }
    }

    /// What either `self` or `later` changed.
    pub(crate) fn or(self, later: Self) -> Self {
        Self {
            triggers: self.triggers || later.triggers,
            aliases: self.aliases || later.aliases,
            prompt: self.prompt || later.prompt,
            macro_groups: self.macro_groups || later.macro_groups,
            groups: self.groups || later.groups,
        }
    }

    /// The lists `profile` and the prompt table on `c` changed since
    /// `before` was read.
    pub(crate) fn since(before: ListRevisions, profile: &Profile, c: &Connection) -> Self {
        Self::between(before, ListRevisions::of(profile, c))
    }

    /// The prompt table alone, for a step that only writes it, such as
    /// following the settings the game sends.
    pub(crate) const PROMPT: Self = Self {
        triggers: false,
        aliases: false,
        prompt: true,
        macro_groups: false,
        groups: false,
    };

    /// The trigger list alone, for a step that only writes triggers.
    pub(crate) const TRIGGERS: Self = Self {
        triggers: true,
        aliases: false,
        prompt: false,
        macro_groups: false,
        groups: false,
    };

    /// The alias list alone.
    pub(crate) const ALIASES: Self = Self {
        triggers: false,
        aliases: true,
        prompt: false,
        macro_groups: false,
        groups: false,
    };

    /// The events these changes send, triggers first.
    pub(crate) fn events(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.triggers {
            out.push(TRIGGERS_CHANGED);
        }
        if self.aliases {
            out.push(ALIASES_CHANGED);
        }
        if self.prompt {
            out.push(PROMPT_CONFIG_CHANGED);
        }
        if self.macro_groups {
            out.push(MACRO_GROUPS_CHANGED);
        }
        if self.groups {
            out.push(GROUPS_CHANGED);
        }
        out
    }
}

/// Send one event to every window for each list of `open`, the profile
/// a step changed, that changed, while it is the profile in front. Call
/// it after the profile lock is released.
pub(crate) fn broadcast_list_changes<R: tauri::Runtime>(
    app: &AppHandle<R>,
    open: &Arc<OpenProfile>,
    changes: ListChanges,
) {
    let events = changes.events();
    if events.is_empty() || !in_front(app, open) {
        return;
    }
    for event in events {
        if event == PROMPT_CONFIG_CHANGED {
            let profile = open.name();
            broadcast(app, event, &PromptConfigChanged { profile });
        } else {
            broadcast(app, event, &"");
        }
    }
}

/// A pane tree as the frontend receives it: the layout plus the
/// [`AppState::panes_generation`] it was read at. The generation never reaches
/// disk, and an inactive profile's tree carries none, since no pane
/// layout write can target it.
#[derive(Clone, serde::Serialize)]
pub(crate) struct PaneLayoutEnvelope {
    #[serde(flatten)]
    pub(crate) layout: PaneLayoutPersist,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) generation: Option<u64>,
}

/// The pane layout of `p`, a profile a session plays, and the panes
/// generation in `state`. Call with the profile lock held.
pub(crate) fn pane_layout_envelope(state: &AppState, p: &Profile) -> PaneLayoutEnvelope {
    PaneLayoutEnvelope {
        layout: p.ui.pane_layout(),
        generation: Some(state.panes_generation()),
    }
}

/// How the Affects pane draws, as every window hears it. Read from the
/// live profile's `[ui]`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AffectsDisplay {
    pub(crate) style: String,
    pub(crate) marker: String,
    pub(crate) tint: bool,
    /// `affects_running_out_hours`.
    pub(crate) running_out: u32,
    /// `affects_almost_gone_hours`.
    pub(crate) almost_gone: u32,
}

impl AffectsDisplay {
    pub(crate) fn of(ui: &crate::profile::ui::UiConfig) -> Self {
        Self {
            style: ui.affects_style.clone(),
            marker: ui.affects_marker.clone(),
            tint: ui.affects_tint,
            running_out: ui.affects_running_out_hours,
            almost_gone: ui.affects_almost_gone_hours,
        }
    }
}

/// What [`broadcast_profile_ui`] hands every window, read from the live
/// profile.
pub(crate) struct ProfileUiEvents {
    pub(crate) panes: PaneLayoutEnvelope,
    pub(crate) tracked: Vec<crate::profile::ui::TrackedAffect>,
    pub(crate) tick_count: String,
    pub(crate) game_time: String,
    pub(crate) chip_style: String,
    pub(crate) affects_display: AffectsDisplay,
    pub(crate) chat_colors: std::collections::BTreeMap<String, String>,
    pub(crate) tick: TickConfig,
}

impl ProfileUiEvents {
    /// Every event [`broadcast_profile_ui`] sends, in order, with its
    /// payload. The replace notice goes last, so a window that reads
    /// the config again on it finds the stores these feed current.
    pub(crate) fn events(&self) -> Vec<(&'static str, serde_json::Value)> {
        [
            event_json(PANE_LAYOUT_CHANGED, &self.panes),
            event_json(TRACKED_AFFECTS_CHANGED, &self.tracked),
            event_json(TICK_COUNT_CHANGED, &self.tick_count),
            event_json(GAME_TIME_CHANGED, &self.game_time),
            event_json(CHIP_STYLE_CHANGED, &self.chip_style),
            event_json(AFFECTS_DISPLAY_CHANGED, &self.affects_display),
            event_json(CHAT_COLORS_CHANGED, &self.chat_colors),
            event_json(TICK_CONFIG_CHANGED, &self.tick),
            Some((UI_CONFIG_REPLACED, serde_json::Value::Null)),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// `payload` as JSON for `event`, or `None` with a warning when it does
/// not serialize, so the rest still go out.
fn event_json<S: serde::Serialize>(
    event: &'static str,
    payload: &S,
) -> Option<(&'static str, serde_json::Value)> {
    match serde_json::to_value(payload) {
        Ok(value) => Some((event, value)),
        Err(e) => {
            warn!(error = %e, event, "broadcast payload did not serialize");
            None
        }
    }
}

/// Read the events [`broadcast_profile_ui`] sends. Call with the profile
/// lock held.
pub(crate) fn profile_ui_events(state: &AppState, p: &Profile) -> ProfileUiEvents {
    ProfileUiEvents {
        panes: pane_layout_envelope(state, p),
        tracked: p.ui.tracked_affects.clone(),
        tick_count: p.ui.tick_count.clone(),
        game_time: p.ui.game_time.clone(),
        chip_style: p.ui.chip_style.clone(),
        affects_display: AffectsDisplay::of(&p.ui),
        chat_colors: p.ui.chat_colors.clone(),
        tick: p.tick.config.clone(),
    }
}

/// Hand every window the panes, tracked affects, tick settings, game
/// time clock, chip style, affects display, and chat colors of the
/// profile in front, the one the selected session plays, then say the
/// UI config was replaced. A switch in the selected session and a
/// selection that brings another profile to the front send them, each of
/// which moves the pane generation as it swaps the panes, and then
/// `vosh://profile-switched`. `#profile load` and `reset` send the same
/// events through [`line_effect_events`], so the status line hears them
/// there too, and Settings reads the new config on
/// [`UI_CONFIG_REPLACED`].
pub(crate) async fn broadcast_profile_ui<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
) {
    let events = {
        let p = state.selected_session().lock_profile().await;
        profile_ui_events(state, &p)
    };
    for (event, payload) in events.events() {
        broadcast(app, event, &payload);
    }
}

/// What every window hears after a run of lines, read from the live
/// profile. After a replace, every window drops its copy of the old
/// panes, tracked affects, and the rest through [`ProfileUiEvents`], so
/// a later panel edit cannot write them back. The tick settings go out
/// with those. Without a replace, a `#tick` command that changed the
/// tick settings sends them alone, so the status line warns with the
/// lead the terminal uses and the Settings Tick card shows it.
pub(crate) fn line_effect_events(
    state: &AppState,
    effects: &crate::input::LineEffects,
    p: &Profile,
) -> Vec<(&'static str, serde_json::Value)> {
    if effects.replaced {
        return profile_ui_events(state, p).events();
    }
    if effects.tick_before.is_some() {
        return event_json(TICK_CONFIG_CHANGED, &p.tick.config)
            .into_iter()
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input;

    fn changes(profile: &mut Profile, c: &mut Connection, line: &str) -> ListChanges {
        let before = ListRevisions::of(profile, c);
        let _ = input::run_line(&AppState::default(), profile, c, line);
        ListChanges::since(before, profile, c)
    }

    #[test]
    fn slash_commands_report_the_list_they_change() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        assert_eq!(
            changes(&mut p, &mut c, "#alias greet wave").events(),
            [ALIASES_CHANGED]
        );
        assert_eq!(
            changes(&mut p, &mut c, "#trigger flee {^You flee} send look").events(),
            [TRIGGERS_CHANGED]
        );
        assert_eq!(
            changes(&mut p, &mut c, "#untrigger flee").events(),
            [TRIGGERS_CHANGED]
        );
        assert_eq!(
            changes(&mut p, &mut c, "#unalias greet").events(),
            [ALIASES_CHANGED]
        );
    }

    #[test]
    fn lua_alias_edits_report_the_alias_list() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        assert_eq!(
            changes(&mut p, &mut c, "#lua mud.alias('k', 'kick')").events(),
            [ALIASES_CHANGED]
        );
        assert_eq!(
            changes(&mut p, &mut c, "#lua mud.unalias('k')").events(),
            [ALIASES_CHANGED]
        );
    }

    #[test]
    fn prompt_commands_report_the_prompt_table() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        assert_eq!(
            changes(&mut p, &mut c, r"#prompt {^<(?<hp>\d+)hp> $}").events(),
            [PROMPT_CONFIG_CHANGED]
        );
        assert_eq!(
            changes(&mut p, &mut c, "#unprompt").events(),
            [PROMPT_CONFIG_CHANGED]
        );
        // With nothing to stop, the table stays as it is.
        let leftover = &changes(&mut p, &mut c, "#unprompt").events();
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            changes(&mut p, &mut c, "#prompt default").events(),
            [PROMPT_CONFIG_CHANGED]
        );
        let leftover = &changes(&mut p, &mut c, "#prompt default").events();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn group_toggles_report_a_macro_group_that_turned() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        p.macros.push(crate::profile::live::Macro {
            key: "F1".into(),
            command: "kick".into(),
            group: Some("combat".into()),
            enabled: true,
            preset: None,
        });
        assert_eq!(
            changes(&mut p, &mut c, "#group combat off").events(),
            [MACRO_GROUPS_CHANGED, GROUPS_CHANGED]
        );
        // Off already, so nothing turned.
        let leftover = &changes(&mut p, &mut c, "#group combat off").events();
        assert!(leftover.is_empty(), "{leftover:?}");
        assert_eq!(
            changes(&mut p, &mut c, "#lua mud.set_group_enabled('combat', true)").events(),
            [MACRO_GROUPS_CHANGED, GROUPS_CHANGED]
        );
        // A group nothing is in leaves the command line alone.
        let leftover = &changes(&mut p, &mut c, "#group nothing off").events();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_group_of_any_list_that_turned_tells_settings() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        let mut kick = vosh_automation::alias::Alias::new("kk", "kick");
        kick.group = Some("combat".into());
        p.aliases.set(kick);
        p.timers.push(crate::profile::live::Timer {
            id: 1,
            name: String::new(),
            interval_secs: 60,
            command: "drink water".into(),
            enabled: true,
            group: Some("upkeep".into()),
        });
        for line in ["#group combat off", "#group upkeep off", "#group upkeep on"] {
            assert_eq!(
                changes(&mut p, &mut c, line).events(),
                [GROUPS_CHANGED],
                "{line}"
            );
        }
        let leftover = &changes(&mut p, &mut c, "#group upkeep on").events();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn steps_that_leave_the_lists_alone_report_nothing() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        let _ = input::process(&mut p, "#alias greet wave");
        let leftover = &changes(&mut p, &mut c, "look").events();
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &changes(&mut p, &mut c, "greet").events();
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &changes(&mut p, &mut c, "#unalias missing").events();
        assert!(leftover.is_empty(), "{leftover:?}");
        let leftover = &changes(&mut p, &mut c, "#aliases").events();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn each_list_sends_its_own_event() {
        assert_eq!(ListChanges::TRIGGERS.events(), [TRIGGERS_CHANGED]);
        assert_eq!(ListChanges::ALIASES.events(), [ALIASES_CHANGED]);
        let all = ListChanges {
            triggers: true,
            aliases: true,
            prompt: true,
            macro_groups: true,
            groups: true,
        };
        assert_eq!(
            all.events(),
            [
                TRIGGERS_CHANGED,
                ALIASES_CHANGED,
                PROMPT_CONFIG_CHANGED,
                MACRO_GROUPS_CHANGED,
                GROUPS_CHANGED
            ]
        );
        assert_eq!(
            ListChanges::TRIGGERS.or(ListChanges::ALIASES).events(),
            [TRIGGERS_CHANGED, ALIASES_CHANGED]
        );
        let leftover = &ListChanges::default().events();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_lua_apply_reports_the_lists_it_changed() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        let outcome = vosh_script::ScriptOutcome {
            actions: vec![vosh_script::Action::SetAlias {
                name: "k".into(),
                expansion: "kick".into(),
            }],
            ..vosh_script::ScriptOutcome::default()
        };
        let apply = crate::script::apply_actions(&mut p, &mut c, outcome);
        assert_eq!(apply.lists, ListChanges::ALIASES);
    }

    #[test]
    fn a_lua_group_toggle_reports_a_macro_group_that_turned() {
        let mut p = Profile::default();
        let mut c = Connection::default();
        p.macros.push(crate::profile::live::Macro {
            key: "F1".into(),
            command: "kick".into(),
            group: Some("combat".into()),
            enabled: true,
            preset: None,
        });
        let toggle = |enabled| vosh_script::ScriptOutcome {
            actions: vec![vosh_script::Action::SetGroupEnabled {
                name: "combat".into(),
                enabled,
            }],
            ..vosh_script::ScriptOutcome::default()
        };
        let apply = crate::script::apply_actions(&mut p, &mut c, toggle(false));
        assert_eq!(apply.lists.events(), [MACRO_GROUPS_CHANGED, GROUPS_CHANGED]);
        let apply = crate::script::apply_actions(&mut p, &mut c, toggle(false));
        let leftover = &apply.lists.events();
        assert!(leftover.is_empty(), "{leftover:?}");
        let apply = crate::script::apply_actions(&mut p, &mut c, toggle(true));
        assert_eq!(apply.lists.events(), [MACRO_GROUPS_CHANGED, GROUPS_CHANGED]);
    }

    /// The payload `events` carries for `name`.
    fn event_payload(events: &super::ProfileUiEvents, name: &str) -> serde_json::Value {
        events
            .events()
            .into_iter()
            .find_map(|(event, payload)| (event == name).then_some(payload))
            .unwrap_or_else(|| panic!("{name} goes out"))
    }

    #[test]
    fn profile_reset_hands_every_window_the_tick_settings_it_put_back() {
        let state = AppState::default();
        let mut profile = crate::profile::live::Profile::default();
        profile.ui.tick_count = "down".into();
        profile.tick.config.warn_at_secs = Some(8);
        profile.tick.config.sound = false;
        let before = super::profile_ui_events(&state, &profile);
        assert_eq!(before.tick_count, "down");
        assert_eq!(before.tick.warn_at_secs, Some(8));

        let ran = crate::input::run_line(
            &state,
            &mut profile,
            &mut crate::session::connection::Connection::default(),
            "#profile reset",
        );
        assert!(ran.replaced);
        let after = super::profile_ui_events(&state, &profile);
        assert_eq!(after.tick_count, "up");
        assert_eq!(after.tick.warn_at_secs, None);
        assert!(after.tick.sound);
    }

    #[test]
    fn a_profile_reset_hands_every_window_the_chip_style_it_put_back() {
        let state = AppState::default();
        let mut profile = crate::profile::live::Profile::default();
        profile.ui.chip_style = "icon_value".into();
        let before = super::profile_ui_events(&state, &profile);
        assert_eq!(
            event_payload(&before, "vosh://chip-style-changed"),
            serde_json::json!("icon_value")
        );

        let ran = crate::input::run_line(
            &state,
            &mut profile,
            &mut crate::session::connection::Connection::default(),
            "#profile reset",
        );
        assert!(ran.replaced);
        let after = super::profile_ui_events(&state, &profile);
        assert_eq!(
            event_payload(&after, "vosh://chip-style-changed"),
            serde_json::json!("value_only")
        );
    }

    #[test]
    fn a_profile_load_or_import_hands_every_window_the_loaded_settings() {
        let state = AppState::default();
        let mut profile = crate::profile::live::Profile::default();
        let mut file = crate::profile::file::ProfileConfig::default();
        file.ui.chip_style = "caption_value".into();
        file.ui.tick_count = "down_past_zero".into();
        file.ui.game_time = "12h".into();
        let _ = file.apply_to(&mut profile);
        let events = super::profile_ui_events(&state, &profile);
        assert_eq!(
            event_payload(&events, "vosh://chip-style-changed"),
            serde_json::json!("caption_value")
        );
        assert_eq!(
            event_payload(&events, "vosh://tick-count-changed"),
            serde_json::json!("down_past_zero")
        );
        assert_eq!(
            event_payload(&events, "vosh://game-time-changed"),
            serde_json::json!("12h")
        );

        // A reset puts back the 24 hour clock.
        let ran = crate::input::run_line(
            &state,
            &mut profile,
            &mut crate::session::connection::Connection::default(),
            "#profile reset",
        );
        assert!(ran.replaced);
        assert_eq!(
            event_payload(
                &super::profile_ui_events(&state, &profile),
                "vosh://game-time-changed"
            ),
            serde_json::json!("24h")
        );
    }

    #[test]
    fn a_profile_load_hands_every_window_the_chat_colors() {
        let state = AppState::default();
        let mut profile = crate::profile::live::Profile::default();
        let mut file = crate::profile::file::ProfileConfig::default();
        file.ui.chat_colors.insert("gtell".into(), "cyan".into());
        let _ = file.apply_to(&mut profile);
        let events = super::profile_ui_events(&state, &profile);
        assert_eq!(
            event_payload(&events, "vosh://chat-colors-changed"),
            serde_json::json!({ "gtell": "cyan" })
        );
    }

    #[test]
    fn a_profile_load_hands_every_window_the_affects_display() {
        let state = AppState::default();
        let mut profile = crate::profile::live::Profile::default();
        let mut file = crate::profile::file::ProfileConfig::default();
        file.ui.affects_style = "chips".into();
        file.ui.affects_marker = "plus_minus".into();
        file.ui.affects_tint = true;
        file.ui.affects_running_out_hours = 5;
        file.ui.affects_almost_gone_hours = 2;
        let _ = file.apply_to(&mut profile);
        let events = super::profile_ui_events(&state, &profile);
        assert_eq!(
            event_payload(&events, "vosh://affects-display-changed"),
            serde_json::json!({
                "style": "chips",
                "marker": "plus_minus",
                "tint": true,
                "running_out": 5,
                "almost_gone": 2,
            })
        );

        let ran = crate::input::run_line(
            &state,
            &mut profile,
            &mut crate::session::connection::Connection::default(),
            "#profile reset",
        );
        assert!(ran.replaced);
        assert_eq!(
            event_payload(
                &super::profile_ui_events(&state, &profile),
                "vosh://affects-display-changed"
            ),
            serde_json::json!({
                "style": "timers",
                "marker": "dot",
                "tint": false,
                "running_out": 2,
                "almost_gone": 1,
            })
        );
    }

    #[test]
    fn the_profile_broadcast_ends_by_saying_the_ui_config_was_replaced() {
        let state = AppState::default();
        let profile = crate::profile::live::Profile::default();
        let names: Vec<&str> = super::profile_ui_events(&state, &profile)
            .events()
            .into_iter()
            .map(|(event, _)| event)
            .collect();
        assert_eq!(
            names,
            [
                "vosh://pane-layout-changed",
                "vosh://tracked-affects-changed",
                "vosh://tick-count-changed",
                "vosh://game-time-changed",
                "vosh://chip-style-changed",
                "vosh://affects-display-changed",
                "vosh://chat-colors-changed",
                "vosh://tick-config-changed",
                crate::app::events::UI_CONFIG_REPLACED,
            ]
        );
        assert_eq!(
            crate::app::events::UI_CONFIG_REPLACED,
            "vosh://ui-config-replaced"
        );
    }

    /// Run `lines` the way the typed path does and hand back what every
    /// window hears after them.
    fn heard_after(
        profile: &mut crate::profile::live::Profile,
        lines: &[&str],
    ) -> Vec<(&'static str, serde_json::Value)> {
        let state = AppState::default();
        let mut c = crate::session::connection::Connection::default();
        let mut effects = crate::input::LineEffects::default();
        for line in lines {
            let ran = crate::input::run_line(&state, profile, &mut c, line);
            effects.note_ran(line, &ran);
        }
        super::line_effect_events(&state, &effects, profile)
    }

    #[test]
    fn a_tick_warn_command_hands_every_window_the_new_lead() {
        let mut profile = crate::profile::live::Profile::default();
        profile.tick.config.warn_at_secs = Some(5);
        let heard = heard_after(&mut profile, &["#tick warn at 10"]);
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(heard[0].0, "vosh://tick-config-changed");
        assert_eq!(heard[0].1["warn_at_secs"], serde_json::json!(10));

        let heard = heard_after(&mut profile, &["#tick warn off", "look"]);
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert_eq!(heard[0].0, "vosh://tick-config-changed");
        assert_eq!(heard[0].1["warn_at_secs"], serde_json::Value::Null);
    }

    #[test]
    fn every_tick_setting_a_command_changes_reaches_every_window() {
        let mut profile = crate::profile::live::Profile::default();
        let heard = heard_after(
            &mut profile,
            &["#tick interval 40", "#tick fire score", "#tick sound off"],
        );
        assert_eq!(heard.len(), 1, "{heard:?}");
        let tick = &heard[0].1;
        assert_eq!(tick["interval_secs"], serde_json::json!(40));
        assert_eq!(tick["auto_fire"], serde_json::json!("score"));
        assert_eq!(tick["sound"], serde_json::json!(false));
    }

    #[test]
    fn lines_that_leave_the_tick_alone_send_nothing() {
        let mut profile = crate::profile::live::Profile::default();
        let leftover = &heard_after(
            &mut profile,
            &["look", "#tick", "#tick warn", "#tick reset"],
        );
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_tick_command_after_a_reset_goes_out_once_with_the_profile() {
        let mut profile = crate::profile::live::Profile::default();
        let heard = heard_after(&mut profile, &["#profile reset", "#tick warn at 10"]);
        let ticks: Vec<_> = heard
            .iter()
            .filter(|(event, _)| *event == "vosh://tick-config-changed")
            .collect();
        assert_eq!(ticks.len(), 1, "{heard:?}");
        assert_eq!(ticks[0].1["warn_at_secs"], serde_json::json!(10));
        assert_eq!(
            heard.last().unwrap().0,
            crate::app::events::UI_CONFIG_REPLACED
        );
    }
}
