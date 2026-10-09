//! The one time check of Line triggers against your prompt. A prompt the
//! profile reads reaches only triggers that match Prompts, so a Line
//! trigger that matched it no longer sees it. Launch runs nothing here.
//! The first session that read your prompt records the check as it ends,
//! under `prompt-line-triggers` in `profiles.toml`, and the next launch
//! names those triggers once.

use vosh_prompt::card::sentences::and_list;

/// The id the check of Line triggers against your prompt is recorded
/// under in `profiles.toml`, after the first session that read a prompt.
pub(crate) const LINE_TRIGGERS: &str = "prompt-line-triggers";

/// The first session with a capture that read your prompt ended. `names`
/// are the enabled Line triggers that matched a prompt it read, which no
/// longer see it. The next launch names them once, and no later session
/// checks again. Nothing is recorded before the profile set loads, so the
/// check waits for a later session.
pub(crate) async fn note_line_triggers(state: &crate::app::state::SharedState, names: Vec<String>) {
    let _persist = crate::disk::save::PERSIST_LOCK.lock().await;
    let mut guard = state.profile_set.lock().await;
    let Some(set) = guard.as_mut() else {
        return;
    };
    let notice = line_trigger_notice(&names);
    if let Err(e) = set.record_with_notice(LINE_TRIGGERS, notice) {
        tracing::error!(error = %e, "could not record the Line triggers that matched your prompt");
    }
}

/// The launch notice that names the Line triggers that matched your
/// prompt. None when none did.
pub(crate) fn line_trigger_notice(names: &[String]) -> Option<String> {
    let (noun, it, them, their) = match names {
        [] => return None,
        [_] => ("trigger", "it", "it", "its"),
        _ => ("triggers", "they", "them", "their"),
    };
    Some(format!(
        "Vosh now sends your prompt only to triggers that match Prompts. The {noun} {} matched \
         your prompt as a line, so {it} no longer {sees} it. Set {their} Match to Prompts in \
         Automation to keep {them} working.",
        and_list(names),
        sees = if names.len() == 1 { "sees" } else { "see" },
    ))
}

#[cfg(test)]
mod tests {
    use super::{line_trigger_notice, note_line_triggers, LINE_TRIGGERS};

    #[test]
    fn the_line_trigger_notice_names_each_trigger() {
        assert_eq!(line_trigger_notice(&[]), None);
        assert_eq!(
            line_trigger_notice(&["hp-watch".to_string()]).as_deref(),
            Some(
                "Vosh now sends your prompt only to triggers that match Prompts. The trigger \
                 hp-watch matched your prompt as a line, so it no longer sees it. Set its Match \
                 to Prompts in Automation to keep it working."
            )
        );
        assert_eq!(
            line_trigger_notice(&["a".to_string(), "b".to_string(), "c".to_string()]).as_deref(),
            Some(
                "Vosh now sends your prompt only to triggers that match Prompts. The triggers \
                 a, b, and c matched your prompt as a line, so they no longer see it. Set their \
                 Match to Prompts in Automation to keep them working."
            )
        );
    }

    #[tokio::test]
    async fn the_first_session_that_read_a_prompt_leaves_the_notice_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state: crate::app::state::SharedState =
            std::sync::Arc::new(crate::app::state::AppState::default());
        // Before the profile set loads, nothing is recorded.
        note_line_triggers(&state, vec!["early".to_string()]).await;
        crate::app::launch::load(&state, root).await;
        let leftover = &state.take_launch_messages();
        assert!(leftover.is_empty(), "{leftover:?}");

        note_line_triggers(&state, vec!["hp-watch".to_string()]).await;
        // A later session checks nothing more.
        note_line_triggers(&state, vec!["other".to_string()]).await;
        let index = std::fs::read_to_string(root.join("profiles.toml")).unwrap();
        assert!(index.contains(LINE_TRIGGERS), "{index}");
        assert!(!index.contains("other"), "{index}");

        // The next launch names it once.
        let next: crate::app::state::SharedState =
            std::sync::Arc::new(crate::app::state::AppState::default());
        crate::app::launch::load(&next, root).await;
        let notices = next.take_launch_messages();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(
            notices[0].contains("The trigger hp-watch matched"),
            "{notices:?}"
        );
        let again: crate::app::state::SharedState =
            std::sync::Arc::new(crate::app::state::AppState::default());
        crate::app::launch::load(&again, root).await;
        let leftover = &again.take_launch_messages();
        assert!(leftover.is_empty(), "{leftover:?}");
    }
}
