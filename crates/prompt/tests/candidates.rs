//! The candidates ring as the card reads it: groups by shape, and the
//! capture check with its match line (section 4).

use vosh_prompt::candidates::{check, groups, shape_of, CaptureCheck};
use vosh_prompt::capture::Recognizer;
use vosh_prompt::config::{AabahranCapture, CaptureConfig, RegexCapture};
use vosh_prompt::stage::Candidate;

const JAMES_PROMPT: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c";

fn entry(id: u64, plain: &str, recognized: bool) -> Candidate {
    Candidate {
        id,
        raw: plain.replace('\n', "\r\n").into_bytes(),
        plain: plain.to_string(),
        at_ms: i64::try_from(id).unwrap() * 1000,
        recognized,
        draw: true,
        capture: true,
    }
}

fn james() -> Recognizer {
    Recognizer::compile(&CaptureConfig::Aabahran(AabahranCapture {
        prompt: JAMES_PROMPT.into(),
        ..AabahranCapture::default()
    }))
    .expect("his prompt compiles")
}

fn ring() -> Vec<Candidate> {
    vec![
        entry(1, "[1020/1020hp 800/800mn 930/930mv]", true),
        entry(2, "You are hungry.", false),
        entry(3, "[980/1020hp 800/800mn 930/930mv]", true),
        entry(
            4,
            "Tester: [===|===|===|---]\n[700/1020hp 790/800mn 930/930mv]",
            true,
        ),
    ]
}

#[test]
fn the_shape_masks_every_run_of_digits() {
    assert_eq!(
        shape_of("[1020/1020hp 800/800mn 930/930mv]  "),
        "[#/#hp #/#mn #/#mv]"
    );
    assert_eq!(shape_of("<-5hp 12m> "), "<#hp #m>");
    assert_eq!(shape_of("a-b 3-4"), "a-b #-#");
}

#[test]
fn candidates_group_by_shape_with_counts() {
    let ring = ring();
    let groups = groups(ring.iter());
    let summary: Vec<(&str, usize, bool)> = groups
        .iter()
        .map(|g| (g.shape.as_str(), g.count, g.recognized))
        .collect();
    assert_eq!(
        summary,
        [
            ("[#/#hp #/#mn #/#mv]", 2, true),
            ("Tester: [===|===|===|---]\n[#/#hp #/#mn #/#mv]", 1, true),
            ("You are hungry.", 1, false),
        ]
    );
    // Newest first inside a group, each with the id the card names it by.
    let ids: Vec<u64> = groups[0].entries.iter().map(|e| e.id).collect();
    assert_eq!(ids, [3, 1]);
    assert_eq!(
        groups[1].entries[0].raw,
        "Tester: [===|===|===|---]\r\n[700/1020hp 790/800mn 930/930mv]"
    );
    assert!(vosh_prompt::candidates::groups(std::iter::empty()).is_empty());
}

#[test]
fn the_check_counts_prompts_fights_and_other_lines() {
    let ring = ring();
    let recognizer = james();
    let scrollback = [
        "You are hungry.",
        // His prompt shown as sent reads like the ring's, so it is no
        // false match.
        "[1020/1020hp 800/800mn 930/930mv]",
        "Tester says '[5/5hp 1/1mn 1/1mv]'",
        "",
    ];
    let result = check(Some(&recognizer), ring.iter(), scrollback.into_iter());
    assert_eq!(
        result,
        CaptureCheck {
            matched: 3,
            total: 4,
            fight_matched: 1,
            false_matches: 0,
            text: "Matches your last 3 prompts and no other line. 1 of them is from a fight."
                .to_string(),
        }
    );
}

#[test]
fn a_loose_pattern_counts_the_lines_it_would_draw_over() {
    let recognizer = Recognizer::compile(&CaptureConfig::Regex(RegexCapture {
        lines: vec![r"(?<hp>\d+)hp".into()],
        ..RegexCapture::default()
    }))
    .expect("a pattern");
    let ring = [entry(1, "<320hp> ", false), entry(2, "<318hp> ", false)];
    let scrollback = [
        "You have 30hp left.",
        "<300hp> ",
        "The guard has 12hp and 4hp.",
    ];
    let result = check(Some(&recognizer), ring.iter(), scrollback.into_iter());
    assert_eq!((result.matched, result.total), (2, 2));
    assert_eq!(result.false_matches, 2);
    assert_eq!(
        result.text,
        "Matches your last 2 prompts and 2 other lines."
    );
}

#[test]
fn the_match_line_says_each_case_in_a_sentence() {
    let line = |matched, total, fight, other| {
        CaptureCheck {
            matched,
            total,
            fight_matched: fight,
            false_matches: other,
            text: String::new(),
        }
        .sentence()
    };
    assert_eq!(
        line(0, 0, 0, 0),
        "Vosh has not seen your prompt since you connected. Send a command and Vosh checks again."
    );
    assert_eq!(
        line(14, 14, 0, 0),
        "Matches your last 14 prompts and no other line."
    );
    assert_eq!(
        line(1, 1, 0, 0),
        "Matches your last prompt and no other line."
    );
    assert_eq!(
        line(14, 14, 3, 0),
        "Matches your last 14 prompts and no other line. 3 of them are from a fight."
    );
    assert_eq!(
        line(1, 3, 1, 1),
        "Matches your last prompt and 1 other line. It is from a fight."
    );
    assert_eq!(
        line(0, 1, 0, 0),
        "Does not match the line before your last command."
    );
    assert_eq!(
        line(0, 5, 0, 0),
        "Does not match any of the 5 lines before your last commands."
    );
}

#[test]
fn with_no_capture_nothing_matches() {
    let ring = ring();
    let result = check(None, ring.iter(), std::iter::empty());
    assert_eq!((result.matched, result.total), (0, 4));
    let empty = check(Some(&james()), std::iter::empty(), std::iter::empty());
    assert_eq!(
        empty.text,
        "Vosh has not seen your prompt since you connected. Send a command and Vosh checks again."
    );
}
