//! The commands for the Affects pane. Each window reads the last
//! affects list and how full each affect was through them. The pane's
//! menu changes how it draws, and the tracked affects editor saves the
//! list of affects it watches for.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::affects::full::FullMap;
use crate::app::events::{AffectsDisplay, AFFECTS_DISPLAY_CHANGED, TRACKED_AFFECTS_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::{save_then_broadcast, SavePolicy};
use crate::profile::inactive::{broadcast_profile_changed, edit_inactive_profile, Stored};
use crate::sessions::SessionId;

/// Replace a profile's tracked affects without touching the rest of
/// its UI config, so an editor outside Settings cannot write a stale
/// snapshot over other fields. Returns the normalized list.
///
/// With no `profile` the selected session's profile takes the list. A
/// `profile` a session plays takes it in memory and saves, and any other
/// has its file rewritten. The profile in front also broadcasts the list
/// as `vosh://tracked-affects-changed` to every window. Any other sends
/// only `vosh://profile-changed`, since the tracked affects event would
/// hand another profile's list to the main window's store.
#[tauri::command]
pub(crate) async fn tracked_affects_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    list: Vec<crate::profile::ui::TrackedAffect>,
    profile: Option<String>,
) -> Result<Vec<crate::profile::ui::TrackedAffect>, String> {
    let list = crate::profile::ui::normalize_tracked_affects(list);
    let shared: SharedState = state.inner().clone();
    let named = match profile.as_deref() {
        Some(name) => match edit_inactive_profile(&shared, name, |_, config| {
            config.ui.tracked_affects.clone_from(&list);
        })
        .await?
        {
            Stored::File(()) => {
                broadcast_profile_changed(&app, name);
                return Ok(list);
            }
            Stored::Open(open) => Some(open),
        },
        None => None,
    };
    let selected = shared.selected_session();
    let open = {
        let mut p = match &named {
            Some(open) => open.lock().await,
            None => selected.lock_profile().await,
        };
        p.ui.tracked_affects.clone_from(&list);
        p.open().clone()
    };
    save_then_broadcast(
        &app,
        &shared,
        &open,
        SavePolicy::Now,
        TRACKED_AFFECTS_CHANGED,
        &list,
    )
    .await;
    if let Some(name) = open.name() {
        broadcast_profile_changed(&app, &name);
    }
    Ok(list)
}

/// What one affects display pick changes. Each field left out stays as
/// it is.
#[derive(Debug, Default)]
pub(crate) struct AffectsDisplayPick {
    pub(crate) style: Option<String>,
    pub(crate) marker: Option<String>,
    pub(crate) tint: Option<bool>,
    pub(crate) running_out: Option<u32>,
    pub(crate) almost_gone: Option<u32>,
}

/// Change how the Affects pane draws without touching the rest of the
/// UI config, for the picks in the pane's own menu. The main window
/// holds no config copy to tell the other windows from, so unlike
/// `ui_set_fields` this sends the new display itself, while the profile
/// is in front. Only what is given changes, and the two thresholds stay
/// in order. Nothing is saved or sent when the pick changes nothing. It
/// writes the profile `profile` names, which a session must play, or the
/// selected session's.
#[tauri::command]
pub(crate) async fn ui_set_affects_display(
    app: AppHandle,
    state: State<'_, SharedState>,
    style: Option<String>,
    marker: Option<String>,
    tint: Option<bool>,
    running_out: Option<u32>,
    almost_gone: Option<u32>,
    profile: Option<String>,
) -> Result<(), String> {
    let pick = AffectsDisplayPick {
        style,
        marker,
        tint,
        running_out,
        almost_gone,
    };
    let (open, changed) = {
        let mut p = state.lock_named(profile).await?;
        let changed = apply_affects_display(&mut p.ui, pick);
        (p.open().clone(), changed)
    };
    let Some(display) = changed else {
        return Ok(());
    };
    save_then_broadcast(
        &app,
        &state,
        &open,
        SavePolicy::Now,
        AFFECTS_DISPLAY_CHANGED,
        &display,
    )
    .await;
    Ok(())
}

/// Write an affects display pick onto the live UI config, coercing an
/// unknown style or marker to the default and the hours to 0 to 99,
/// almost gone never over running out. Returns the new display when
/// anything changed, so an unchanged pick saves and sends nothing.
fn apply_affects_display(
    ui: &mut crate::profile::ui::UiConfig,
    pick: AffectsDisplayPick,
) -> Option<AffectsDisplay> {
    let before = AffectsDisplay::of(ui);
    if let Some(style) = pick.style {
        ui.affects_style = crate::profile::ui::coerce_affects_style(style);
    }
    if let Some(marker) = pick.marker {
        ui.affects_marker = crate::profile::ui::coerce_affects_marker(marker);
    }
    if let Some(tint) = pick.tint {
        ui.affects_tint = tint;
    }
    if let Some(hours) = pick.running_out {
        ui.affects_running_out_hours = hours;
    }
    if let Some(hours) = pick.almost_gone {
        ui.affects_almost_gone_hours = hours;
    }
    (ui.affects_running_out_hours, ui.affects_almost_gone_hours) =
        crate::profile::ui::coerce_affects_thresholds(
            ui.affects_running_out_hours,
            ui.affects_almost_gone_hours,
        );
    let after = AffectsDisplay::of(ui);
    (after != before).then_some(after)
}

/// The last `Char.Affects` payload of the session's connection, raw as
/// the MUD sent it, or null.
#[tauri::command]
pub(crate) async fn affects_snapshot_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<Option<Value>, String> {
    Ok(state
        .session(session)?
        .last_packages
        .get(crate::affects::AFFECTS_PACKAGE))
}

/// The live map of the session's connection, hours at full by affect
/// key.
#[tauri::command]
pub(crate) async fn affect_full_get(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<FullMap, String> {
    Ok(state.session(session)?.affect_full.map())
}

#[cfg(test)]
mod tests {
    use crate::profile::ui::UiConfig;

    #[test]
    fn an_affects_display_pick_writes_only_what_it_names() {
        let mut ui = UiConfig {
            affects_marker: "square".into(),
            affects_tint: true,
            ..UiConfig::default()
        };
        let pick = |style: Option<&str>, marker: Option<&str>, tint: Option<bool>| {
            super::AffectsDisplayPick {
                style: style.map(Into::into),
                marker: marker.map(Into::into),
                tint,
                ..super::AffectsDisplayPick::default()
            }
        };
        let display = super::apply_affects_display(&mut ui, pick(Some("countdown"), None, None))
            .expect("a new style changes the display");
        assert_eq!(display.style, "countdown");
        assert_eq!(display.marker, "square");
        assert!(display.tint);
        assert_eq!(ui.affects_style, "countdown");
        assert_eq!(ui.affects_marker, "square");
        assert!(ui.affects_tint);

        // The same pick again changes nothing, so nothing is saved or sent.
        assert_eq!(
            super::apply_affects_display(&mut ui, pick(Some("countdown"), None, Some(true))),
            None
        );

        // An unknown value coerces to the default before it compares.
        let display =
            super::apply_affects_display(&mut ui, pick(None, Some("sparkle"), Some(false)))
                .expect("the marker and the tint change");
        assert_eq!(display.marker, "dot");
        assert!(!display.tint);
        assert_eq!(ui.affects_style, "countdown");
        assert_eq!(
            super::apply_affects_display(&mut ui, pick(None, Some("dot"), None)),
            None
        );
        // No pick touched the hours.
        assert_eq!(display.running_out, 2);
        assert_eq!(display.almost_gone, 1);
    }

    #[test]
    fn an_affects_threshold_pick_keeps_almost_gone_under_running_out() {
        let mut ui = UiConfig::default();
        let display = super::apply_affects_display(
            &mut ui,
            super::AffectsDisplayPick {
                running_out: Some(5),
                almost_gone: Some(2),
                ..super::AffectsDisplayPick::default()
            },
        )
        .expect("the hours change");
        assert_eq!((display.running_out, display.almost_gone), (5, 2));
        assert_eq!(display.style, "timers");
        // Running out under almost gone pulls almost gone down with it.
        let display = super::apply_affects_display(
            &mut ui,
            super::AffectsDisplayPick {
                running_out: Some(1),
                ..super::AffectsDisplayPick::default()
            },
        )
        .expect("the hours change");
        assert_eq!((display.running_out, display.almost_gone), (1, 1));
        // Almost gone over running out stops at it, which changes nothing.
        assert_eq!(
            super::apply_affects_display(
                &mut ui,
                super::AffectsDisplayPick {
                    almost_gone: Some(9),
                    ..super::AffectsDisplayPick::default()
                },
            ),
            None
        );
        assert_eq!(
            (ui.affects_running_out_hours, ui.affects_almost_gone_hours),
            (1, 1)
        );
    }
}
