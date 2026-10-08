//! The candidates ring as the card reads it: groups by shape, and the
//! capture check with its match line.

use vosh_prompt::capture::Recognizer;
use vosh_prompt::card::candidates::{check, groups, shape_of, CaptureCheck, CheckRead, Mark};
use vosh_prompt::config::{AabahranCapture, CaptureConfig, RegexCapture};
use vosh_prompt::stage::Candidate;
use vosh_prompt::testkit::mud::PROMPT;

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
        prompt: PROMPT.into(),
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
    let leftover = &vosh_prompt::card::candidates::groups(std::iter::empty());
    assert!(leftover.is_empty(), "{leftover:?}");
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
        (
            result.matched,
            result.total,
            result.fight_matched,
            result.false_matches
        ),
        (3, 4, 1, 0)
    );
    assert_eq!(
        result.text,
        "Matches your last 3 prompts and no other line. 1 of them is from a fight."
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
            reads: Vec::new(),
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

fn mark(line: usize, start: usize, end: usize, field: Option<&str>, label: &str) -> Mark {
    Mark {
        line,
        start,
        end,
        field: field.map(str::to_string),
        label: label.to_string(),
        warn: false,
    }
}

/// The text each mark covers, by line, for reading a test at a glance.
fn covered(read: &CheckRead) -> Vec<(usize, String, String)> {
    let lines: Vec<&str> = read.plain.split('\n').collect();
    read.marks
        .iter()
        .map(|m| {
            let text: String = lines[m.line]
                .chars()
                .skip(m.start)
                .take(m.end - m.start)
                .collect();
            (m.line, text, m.label.clone())
        })
        .collect()
}

#[test]
fn the_check_marks_every_value_each_prompt_it_reads_holds_newest_first() {
    let ring = ring();
    let result = check(Some(&james()), ring.iter(), std::iter::empty());
    let ids: Vec<u64> = result.reads.iter().map(|r| r.id).collect();
    assert_eq!(ids, [4, 3, 1]);
    let fight: Vec<bool> = result.reads.iter().map(|r| r.fight).collect();
    assert_eq!(fight, [true, false, false]);
    let quiet = &result.reads[1];
    assert_eq!(quiet.plain, "[980/1020hp 800/800mn 930/930mv]");
    assert_eq!(quiet.raw, "[980/1020hp 800/800mn 930/930mv]");
    assert_eq!(quiet.at_ms, 3000);
    assert_eq!(
        quiet.marks,
        [
            mark(0, 1, 4, Some("hp"), "Health"),
            mark(0, 5, 9, Some("maxhp"), "Max health"),
            mark(0, 12, 15, Some("mana"), "Mana"),
            mark(0, 16, 19, Some("maxmana"), "Max mana"),
            mark(0, 22, 25, Some("move"), "Moves"),
            mark(0, 26, 29, Some("maxmove"), "Max moves"),
        ]
    );
    // A fight prompt marks the tank and the bar inside its brackets, then
    // the vitals line under it.
    let tank = covered(&result.reads[0]);
    assert_eq!(
        tank[..2],
        [
            (0, "Tester".to_string(), "Tank".to_string()),
            (0, "===|===|===|---".to_string(), "Tank health".to_string()),
        ]
    );
    assert_eq!(tank[2], (1, "700".to_string(), "Health".to_string()));
    assert_eq!(tank.len(), 8);
}

#[test]
fn the_immortal_prefix_marks_its_two_levels() {
    let ring = [entry(
        1,
        "(Wizi 60) (Incog 60) [1020/1020hp 800/800mn 930/930mv]",
        true,
    )];
    let result = check(Some(&james()), ring.iter(), std::iter::empty());
    let marks = covered(&result.reads[0]);
    assert_eq!(
        marks[..3],
        [
            (0, "60".to_string(), "Wizi".to_string()),
            (0, "60".to_string(), "Incog".to_string()),
            (0, "1020".to_string(), "Health".to_string()),
        ]
    );
    assert_eq!(result.reads[0].marks[0].start, 6);
    assert_eq!(result.reads[0].marks[1].start, 17);
    assert_eq!(result.reads[0].marks[0].field.as_deref(), Some("wizi"));
}

#[test]
fn codes_that_run_together_mark_their_run_as_one_warning() {
    let healer = Recognizer::compile(&CaptureConfig::Aabahran(AabahranCapture {
        prompt: "<%h%m %vmv> ".into(),
        ..AabahranCapture::default()
    }))
    .expect("the codes compile");
    let ring = [entry(1, "<1020800 930mv> ", true)];
    let result = check(Some(&healer), ring.iter(), std::iter::empty());
    assert_eq!(
        result.reads[0].marks,
        [
            Mark {
                warn: true,
                ..mark(0, 1, 8, None, "Health and Mana")
            },
            mark(0, 9, 12, Some("move"), "Moves"),
        ]
    );
}

#[test]
fn a_pattern_marks_each_group_by_the_value_it_feeds() {
    let mut names = std::collections::BTreeMap::new();
    names.insert("2".to_string(), "maxhp".to_string());
    names.insert("3".to_string(), String::new());
    let recognizer = Recognizer::compile(&CaptureConfig::Regex(RegexCapture {
        lines: vec![r"^<(?<hp>\d+)/(\d+)hp (\d+)m (?<sp>\d+)sp> +$".into()],
        names,
        settle: true,
        ..RegexCapture::default()
    }))
    .expect("a pattern");
    let ring = [entry(1, "<12/40hp 3m 9sp> ", false)];
    let result = check(Some(&recognizer), ring.iter(), std::iter::empty());
    assert_eq!(
        result.reads[0].marks,
        [
            mark(0, 1, 3, Some("hp"), "Health"),
            mark(0, 4, 6, Some("maxhp"), "Max health"),
            mark(0, 12, 13, Some("sp"), "sp"),
        ]
    );
}

#[test]
fn a_character_of_several_bytes_counts_once_in_a_mark() {
    let recognizer = Recognizer::compile(&CaptureConfig::Regex(RegexCapture {
        lines: vec![r"^é (?<hp>\d+)hp$".into()],
        ..RegexCapture::default()
    }))
    .expect("a pattern");
    let ring = [entry(1, "é 50hp", false)];
    let result = check(Some(&recognizer), ring.iter(), std::iter::empty());
    assert_eq!(result.reads[0].marks, [mark(0, 2, 4, Some("hp"), "Health")]);
}
