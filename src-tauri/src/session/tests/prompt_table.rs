//! The profile keeps the `[prompt]` table the prompt engine on the
//! connection holds, which is the copy its file saves.

use std::sync::Arc;

use serde_json::json;
use vosh_prompt::config::AabahranCapture;
use vosh_prompt::{CaptureConfig, PromptConfig, PromptShow};

use super::*;
use crate::app::state::{AppState, SharedState};
use crate::profile::file::ProfileConfig;
use crate::profile::set::ProfileSet;

/// A table with no design of its own, drawing on, that reads `prompt`
/// and follows the game.
fn mirroring(prompt: &str) -> PromptConfig {
    PromptConfig {
        draw: true,
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::fresh()
    }
}

/// The codes the capture reads.
fn codes(config: &PromptConfig) -> &str {
    match &config.capture {
        CaptureConfig::Aabahran(codes) => &codes.prompt,
        other => panic!("an aabahran capture, got {other:?}"),
    }
}

fn packet(package: &str, data: serde_json::Value) -> vosh_protocol::gmcp::Message {
    vosh_protocol::gmcp::Message {
        package: package.into(),
        data,
    }
}

/// Char.Status for an immortal.
fn immortal() -> vosh_protocol::gmcp::Message {
    packet(
        "Char.Status",
        json!({"name": "Tester", "level": 60, "race": "human", "class": "warrior"}),
    )
}

/// A color left open right before `%u`, which a mortal's design cannot
/// draw and an immortal's can, so who you are writes another design.
const MOVED: &str = "<`(12%u %h> ";

#[tokio::test]
async fn the_profile_keeps_the_table_the_engine_holds() {
    let dir = tempfile::tempdir().unwrap();
    let state: SharedState = Arc::new(AppState::default());
    let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
    set.create_from("Healer", None, None).unwrap();
    // Healer's file holds a table read before your codes moved.
    let mut file = ProfileConfig::default();
    file.set_prompt(mirroring("<%hhp> "));
    file.save(&set.profile_path("Healer")).unwrap();
    *state.profile_set.lock().await = Some(set);
    let now = Instant::now();
    let same = |p: &Profile, c: &Connection| assert_eq!(p.prompt, *c.prompt.config());

    {
        let mut p = state.profile.lock().await;
        let mut c = state.connection.lock();
        start_prompt(&mut p, &mut c, true);
        // A Settings save.
        crate::prompt::set_config(&mut p, &mut c, mirroring("<%hhp> ")).unwrap();
        same(&p, &c);
        // A #prompt edit.
        let _ = crate::input::run_line(&state, &mut p, &mut c, "#prompt show pinned");
        assert_eq!(p.prompt.show, PromptShow::Pinned);
        same(&p, &c);
        // A Char.Prompt the capture follows.
        let _ = gmcp_step(
            &mut p,
            &mut c,
            &packet(
                "Char.Prompt",
                json!({"enabled": true, "prompt": MOVED, "fprompt": ""}),
            ),
            now,
        );
        assert_eq!(codes(&p.prompt), MOVED);
        same(&p, &c);
        // A Char.Status that says you are an immortal writes the design
        // again.
        let mortal = p.prompt.template.clone();
        let _ = gmcp_step(&mut p, &mut c, &immortal(), now);
        assert_ne!(p.prompt.template, mortal);
        same(&p, &c);
    }

    // A switch hands over Healer's table, and the latest Char.Prompt
    // moves its codes.
    crate::profile::switch::switch_live_profile(&state, "Healer")
        .await
        .unwrap();
    let mut p = state.profile.lock().await;
    let mut c = state.connection.lock();
    assert_eq!(codes(&p.prompt), MOVED);
    same(&p, &c);

    // A connect starts as a mortal again.
    let immortal_design = p.prompt.template.clone();
    start_prompt(&mut p, &mut c, true);
    assert_ne!(p.prompt.template, immortal_design);
    same(&p, &c);

    // So does a disconnect.
    let _ = gmcp_step(&mut p, &mut c, &immortal(), now);
    assert_eq!(p.prompt.template, immortal_design);
    end_prompt(&mut p, &mut c);
    assert_ne!(p.prompt.template, immortal_design);
    same(&p, &c);
}
