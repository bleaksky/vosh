//! Where each piece of your prompt lands on screen, for the prompt card's
//! pointer mapping.
//!
//! The webview maps a pointer to a piece from the open row's spans and
//! plain rows, laid out from the region's start at the renderer's width
//! with the shared word wrap. These cases play the session's own steps
//! and keep what the webview reads: the payloads, the open row the card
//! reads, the band's zone, and for the native grid its screen and its
//! cursor report at three widths. The webview test replays the payloads
//! into xterm as well, and holds the layout to the characters both
//! renderers drew and to the dock's own grid.

use super::*;
use crate::output::OutputPayload;
use vosh_prompt::PromptShow;

/// A design of one row, which word wraps at a narrow width.
const ONE_ROW: &str = "[%hp/%{maxhp}hp %mana/%{maxmana}mn %move/%{maxmove}mv] ";
/// A design of two rows, colored.
const TWO_ROWS: &str = "%{c:2}HP%c_default %hp%nl<%mana mn %move mv> ";

/// The widths the native grid lays each case out at.
const WIDTHS: [usize; 3] = [80, 30, 12];

/// One case: two pulses, a quiet prompt and then a fight with a tank, so
/// the first prompt is history the second one left behind. What the
/// webview reads after them.
fn case(template: &str, show: PromptShow) -> serde_json::Value {
    let mut session = Session::new(showing(profile(CODES, template, true), show));
    let reads = [
        session.read(&wire_fixture("quiet")),
        session.read(&wire_fixture("fight-tank")),
    ];
    let payloads: Vec<serde_json::Value> = reads
        .iter()
        .filter(|read| !read.out.is_empty())
        .map(|read| serde_json::to_value(OutputPayload::from_output(&read.out)).expect("json"))
        .collect();
    let state = crate::prompt::prompt_state(&session.p, &session.c);
    let native: serde_json::Map<String, serde_json::Value> = WIDTHS
        .into_iter()
        .filter(|_| show != PromptShow::Pinned)
        .map(|columns| {
            let mut grid = crate::native::grid::TermGrid::new(columns, 24);
            for read in &reads {
                grid.session_output(&read.out);
            }
            let screen = rows_of(&grid);
            let cursor = grid.cursor_report();
            grid.local_write(b"look");
            (
                columns.to_string(),
                serde_json::json!({
                    "screen": screen,
                    "cursor": cursor,
                    "after_echo": grid.cursor_report(),
                }),
            )
        })
        .collect();
    serde_json::json!({
        "template": template,
        "show": show.name(),
        "payloads": payloads,
        "open_row": state.open_row,
        "zone": session.c.prompt.zone(),
        "native": native,
    })
}

/// Every case the webview test plays.
fn pointer_cases() -> serde_json::Value {
    let mut cases = Vec::new();
    for template in [HP, ONE_ROW, TWO_ROWS] {
        for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
            cases.push(case(template, show));
        }
    }
    serde_json::json!({
        "notes": "Generated from the session's own steps on the synthetic wire fixtures quiet.bin and fight-tank.bin. Written again with VOSH_WRITE_POINTER_CASES=1.",
        "cases": cases,
    })
}

fn pointer_cases_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/prompt/aabahran/pointer/cases.json")
}

/// Write [`pointer_cases`] for the webview test when
/// `VOSH_WRITE_POINTER_CASES` is set. Nothing otherwise.
#[test]
fn write_the_pointer_cases_for_the_webview() {
    if std::env::var("VOSH_WRITE_POINTER_CASES").is_err() {
        return;
    }
    let mut text = serde_json::to_string_pretty(&pointer_cases()).expect("json");
    text.push('\n');
    let path = pointer_cases_path();
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
    std::fs::write(&path, text).expect("the file");
}

#[test]
fn the_pointer_cases_the_webview_plays_are_what_the_session_sends() {
    let stored = std::fs::read_to_string(pointer_cases_path()).expect(
        "fixtures/prompt/aabahran/pointer/cases.json, written with VOSH_WRITE_POINTER_CASES=1",
    );
    let stored: serde_json::Value = serde_json::from_str(&stored).expect("json");
    assert!(
        stored == pointer_cases(),
        "the session changed, so write the file again with VOSH_WRITE_POINTER_CASES=1"
    );
}

#[test]
fn every_case_leaves_an_open_row_whose_region_the_native_grid_finds() {
    for template in [HP, ONE_ROW, TWO_ROWS] {
        for show in [PromptShow::Text, PromptShow::Lifted] {
            let case = case(template, show);
            let gen = &case["open_row"]["gen"];
            assert!(gen.is_u64(), "{template} {show:?}: an open row");
            for columns in WIDTHS {
                let native = &case["native"][columns.to_string()];
                assert_eq!(
                    &native["cursor"]["region"]["gen"], gen,
                    "{template} {show:?} {columns} wide"
                );
                assert!(native["after_echo"]["region"].is_null());
            }
        }
        // Pinned, the band holds the design, and the text holds no row.
        let case = case(template, PromptShow::Pinned);
        assert!(case["open_row"].is_null());
        let last = case["payloads"]
            .as_array()
            .and_then(|p| p.last())
            .expect("a payload");
        assert!(
            last["pin_spans"].is_array(),
            "{template}: the band's pieces"
        );
    }
}
