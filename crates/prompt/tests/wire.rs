//! The synthetic pulses in `fixtures/prompt/aabahran/wire` are what the
//! test kit's fake plays. After a change to
//! the fake, run this test with `VOSH_WRITE_WIRE=1` to write them again,
//! then read the diff.
//!
//! The session tests in `src-tauri` read each file through the session at
//! every split.

use vosh_prompt::aabahran::{compile, Origin, Who};
use vosh_prompt::testkit::shown;
use vosh_prompt::testkit::wire::{self, CASES};

const DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/prompt/aabahran/wire"
);

#[test]
fn every_wire_fixture_is_what_the_fake_plays() {
    let write = std::env::var_os("VOSH_WRITE_WIRE").is_some();
    for case in CASES {
        let path = format!("{DIR}/{}.bin", case.name);
        let played = wire::play(case.name).expect("every case plays");
        if write {
            std::fs::write(&path, &played).expect("the fixture writes");
            continue;
        }
        let saved = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            saved == played,
            "{} is not what the fake plays. Run with VOSH_WRITE_WIRE=1 and read the diff.",
            case.name
        );
    }
    assert_eq!(wire::play("no such case"), None);
}

#[test]
fn every_wire_fixture_has_notes_that_mark_it_synthetic() {
    let mut want: Vec<String> = CASES
        .iter()
        .flat_map(|c| [format!("{}.bin", c.name), format!("{}.notes.md", c.name)])
        .collect();
    want.sort();
    let mut have: Vec<String> = std::fs::read_dir(DIR)
        .expect("the wire folder")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    have.sort();
    assert_eq!(
        have, want,
        "one .bin and one .notes.md per case, nothing else"
    );
    for case in CASES {
        let notes = std::fs::read_to_string(format!("{DIR}/{}.notes.md", case.name))
            .expect("the notes read");
        assert!(
            notes.contains("Synthetic.") && notes.contains(&format!("`{}`", case.name)),
            "{} notes",
            case.name
        );
    }
}

#[test]
fn every_wire_prompt_reads_back_with_the_prompt_of_its_case() {
    for case in CASES {
        let bytes = wire::play(case.name).expect("every case plays");
        let text = shown(&bytes);
        if case.name == "prompts-off-new" {
            assert!(!text.contains("hp"), "no prompt prints: {text:?}");
            continue;
        }
        let block = text.rsplit("\n\r\n\r").next().expect("a prompt block");
        let mut lines: Vec<&str> = block.split("\n\r").collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        let compiled =
            compile(case.prompt, "", Origin::Stored, Who::default()).expect("it compiles");
        let read = compiled
            .shapes
            .iter()
            .find_map(|s| s.read(&lines).or_else(|| s.read_partial(&lines)))
            .unwrap_or_else(|| panic!("{}: nothing reads {lines:?}", case.name));
        let want_hp = match case.name {
            "fight-tank" => "765",
            n if n.starts_with("lament") => "0",
            _ => "1020",
        };
        assert_eq!(read.values["hp"], want_hp, "{}", case.name);
    }
}
