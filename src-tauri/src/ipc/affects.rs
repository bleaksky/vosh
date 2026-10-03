//! The commands for the Affects pane. Each window reads the last
//! affects list and how full each affect was through them. The pane's
//! menu changes how it draws, and the tracked affects editor saves the
//! list of affects it watches for.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::affect_full::FullMap;
use crate::app::events::{
    broadcast, AffectsDisplay, AFFECTS_DISPLAY_CHANGED, TRACKED_AFFECTS_CHANGED,
};
use crate::app::state::SharedState;
use crate::disk::save::persist_profile;

/// Replace a profile's tracked affects without touching the rest of
/// its UI config, so an editor outside Settings cannot write a stale
/// snapshot over other fields. Returns the normalized list.
///
/// With no `profile`, or the active one, the live profile takes the
/// list, persists, and broadcasts it as `vosh://tracked-affects-changed`
/// to every window. An inactive `profile` has its file rewritten
/// instead, and only `vosh://profile-changed` goes out, since the
/// tracked affects event would hand another profile's list to the main
/// window's store.
#[tauri::command]
pub(crate) async fn tracked_affects_set(
    app: AppHandle,
    state: State<'_, SharedState>,
    list: Vec<crate::profile_config::TrackedAffect>,
    profile: Option<String>,
) -> Result<Vec<crate::profile_config::TrackedAffect>, String> {
    let list = crate::profile_config::normalize_tracked_affects(list);
    let shared: SharedState = state.inner().clone();
    if let Some(name) = profile.as_deref() {
        let written = crate::characters::edit_inactive_profile(&shared, name, |_, config| {
            config.ui.tracked_affects.clone_from(&list);
        })
        .await?;
        if written.is_some() {
            crate::characters::broadcast_profile_changed(&app, name);
            return Ok(list);
        }
    }
    {
        let mut p = state.profile.lock().await;
        p.ui.tracked_affects.clone_from(&list);
    }
    persist_profile(&app, &shared).await;
    broadcast(&app, TRACKED_AFFECTS_CHANGED, &list);
    if let Some(active) = crate::characters::active_name(&shared).await {
        crate::characters::broadcast_profile_changed(&app, &active);
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
/// holds no whole config to save, and Settings may hold one with newer
/// fields, so a whole config write from either would put stale values
/// back. Only what is given changes. Nothing is saved or sent when the
/// pick changes nothing.
#[tauri::command]
pub(crate) async fn ui_set_affects_display(
    app: AppHandle,
    state: State<'_, SharedState>,
    style: Option<String>,
    marker: Option<String>,
    tint: Option<bool>,
    running_out: Option<u32>,
    almost_gone: Option<u32>,
) -> Result<(), String> {
    let pick = AffectsDisplayPick {
        style,
        marker,
        tint,
        running_out,
        almost_gone,
    };
    let changed = {
        let mut p = state.profile.lock().await;
        apply_affects_display(&mut p.ui, pick)
    };
    let Some(display) = changed else {
        return Ok(());
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&app, &shared).await;
    broadcast(&app, AFFECTS_DISPLAY_CHANGED, &display);
    Ok(())
}

/// Write an affects display pick onto the live UI config, coercing an
/// unknown style or marker to the default and the hours to 0 to 99,
/// almost gone never over running out. Returns the new display when
/// anything changed, so an unchanged pick saves and sends nothing.
fn apply_affects_display(
    ui: &mut crate::profile_config::UiConfig,
    pick: AffectsDisplayPick,
) -> Option<AffectsDisplay> {
    let before = AffectsDisplay::of(ui);
    if let Some(style) = pick.style {
        ui.affects_style = crate::profile_config::coerce_affects_style(style);
    }
    if let Some(marker) = pick.marker {
        ui.affects_marker = crate::profile_config::coerce_affects_marker(marker);
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
        crate::profile_config::coerce_affects_thresholds(
            ui.affects_running_out_hours,
            ui.affects_almost_gone_hours,
        );
    let after = AffectsDisplay::of(ui);
    (after != before).then_some(after)
}

/// The last `Char.Affects` payload of this connection, raw as the MUD
/// sent it, or null.
#[tauri::command]
pub(crate) async fn affects_snapshot_get(
    state: State<'_, SharedState>,
) -> Result<Option<Value>, String> {
    Ok(state.last_affects.get())
}

/// The live map, hours at full by affect key.
#[tauri::command]
pub(crate) async fn affect_full_get(state: State<'_, SharedState>) -> Result<FullMap, String> {
    Ok(state.affect_full.map())
}

#[cfg(test)]
mod tests {
    use crate::profile_config::UiConfig;

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
