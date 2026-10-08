//! The commands for your edits to the presets: read the
//! `[preset_edits]` table of a profile, and save
//! the rows one preset's card or one preset trigger's card changed.

use tauri::{AppHandle, State};

use crate::app::events::{broadcast, PresetEditsChanged, PRESET_EDITS_CHANGED};
use crate::app::state::SharedState;
use crate::disk::save::{save_by, SavePolicy};
use crate::loadouts::preset_edits::{merge, PresetEdit, PresetEdits};

/// Your edits to the presets of `profile`, or of the selected session's
/// profile when it names none. In loadout mode every profile holds the
/// catalog's.
#[tauri::command]
pub(crate) async fn preset_edits_get(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<PresetEdits, String> {
    Ok(state.lock_named(profile).await?.preset_edits.clone())
}

/// Merge `edits`, the rows the page changed in the preset `id`, into the
/// table of `profile`, or of the selected session's profile when it names
/// none, save, and tell every window. See [`merge`] for what each row
/// keeps.
#[tauri::command]
pub(crate) async fn preset_edits_set<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    id: String,
    edits: PresetEdit,
    profile: Option<String>,
) -> Result<(), String> {
    let open = {
        let mut p = state.lock_named(profile).await?;
        merge(&mut p.preset_edits, &id, edits);
        p.open().clone()
    };
    save_by(&app, &state, &open, SavePolicy::Now).await;
    let profile = open.name();
    broadcast(&app, PRESET_EDITS_CHANGED, &PresetEditsChanged { profile });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::{Listener, Manager};

    use super::*;
    use crate::app::state::AppState;
    use crate::loadouts::preset_edits::EditRow;

    fn line(value: &str, was: &str) -> PresetEdit {
        PresetEdit {
            colors: BTreeMap::from([(
                "line".into(),
                EditRow {
                    value: value.into(),
                    was: was.into(),
                    seen: None,
                },
            )]),
            ..PresetEdit::default()
        }
    }

    #[tokio::test]
    async fn a_set_merges_into_the_table_and_tells_every_window() {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let into = heard.clone();
        app.listen_any(PRESET_EDITS_CHANGED, move |event| {
            into.lock().unwrap().push(event.payload().to_string());
        });
        let set = |edit| {
            preset_edits_set(
                app.handle().clone(),
                app.state(),
                "disarm_buff_fade".into(),
                edit,
                None,
            )
        };
        assert_eq!(set(line("#c3a6ff", "fg:178")).await, Ok(()));
        assert_eq!(set(line("#ffaf00", "fg:214")).await, Ok(()));
        let table = preset_edits_get(app.state(), None).await.unwrap();
        assert_eq!(
            table["disarm_buff_fade"],
            line("#ffaf00", "fg:178"),
            "the second set keeps the first was"
        );
        assert_eq!(heard.lock().unwrap().len(), 2);
        assert_eq!(set(line("fg:178", "fg:178")).await, Ok(()));
        let table = preset_edits_get(app.state(), None).await.unwrap();
        assert!(table.is_empty(), "{table:?}");
    }
}
