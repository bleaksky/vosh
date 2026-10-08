//! A capture built from a line another game prints.

use std::collections::BTreeMap;

use vosh_prompt::capture::generic::{from_line, Generic};
use vosh_prompt::capture::{fills, Recognizer};
use vosh_prompt::config::CaptureConfig;

fn generic(line: &str, names: &[&str]) -> Generic {
    let names: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
    from_line(line, &names)
}

fn suggested(line: &str) -> Vec<String> {
    generic(line, &[])
        .numbers
        .iter()
        .map(|n| n.suggested.clone())
        .collect()
}

fn pattern(line: &str, names: &[&str]) -> String {
    generic(line, names).capture.lines[0].clone()
}

fn recognizer(generic: &Generic) -> Recognizer {
    Recognizer::compile(&CaptureConfig::Regex(generic.capture.clone())).expect("it compiles")
}

#[test]
fn numbers_take_their_names_from_the_letters_after_them() {
    assert_eq!(suggested("<100hp 50m 30mv> "), ["hp", "mana", "move"]);
    assert_eq!(
        suggested("[100/120hp 50/60mn 30/40mv]"),
        ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"]
    );
    assert_eq!(suggested("100H 50SP 30ST > "), ["hp", "mana", "move"]);
    assert_eq!(
        suggested("<12hit 4ma 9mov 3move> "),
        ["hp", "mana", "move", "n1"]
    );
    // Letters before a number name nothing, and neither does a value
    // named twice.
    assert_eq!(
        suggested("HP:100 SP:50 (3) 7hp 8hp"),
        ["n1", "n2", "n3", "hp", "n4"]
    );
    // A pair with no letters after it.
    assert_eq!(suggested("12/34 > "), ["n1", "n2"]);
    let numbers = generic("[100/120hp]", &[]).numbers;
    assert!(!numbers[0].max && numbers[1].max);
    assert_eq!((numbers[0].span, numbers[0].text.as_str()), ([1, 4], "100"));
    assert_eq!((numbers[1].span, numbers[1].text.as_str()), ([5, 8], "120"));
    assert_eq!(numbers[0].label, "Health");
    assert_eq!(numbers[1].label, "Max health");
    assert_eq!(numbers[0].name, "hp");
}

#[test]
fn the_line_becomes_one_anchored_pattern() {
    assert_eq!(
        pattern("<100hp 50m 30mv> ", &[]),
        r"^<(?<hp>-?\d+)hp +(?<mana>-?\d+)m +(?<move>-?\d+)mv> +$"
    );
    // Text is escaped, a sign belongs to its number, and every run of
    // spaces matches any run of spaces, at the end of the line too.
    assert_eq!(
        pattern("[-5hp]  (x)  ", &[]),
        r"^\[(?<hp>-?\d+)hp\] +\(x\) +$"
    );
    // A minus sign after a digit or a letter is text.
    assert_eq!(pattern("3-4", &[]), r"^(?<n1>-?\d+)\-(?<n2>-?\d+)$");
    assert_eq!(suggested("a-5"), ["n1"]);
    assert_eq!(generic("a-5", &[]).numbers[0].text, "5");
}

#[test]
fn the_capture_reads_the_values_and_settles_on_its_last_character() {
    let line = generic("<100hp 50m 30mv> ", &[]);
    assert!(line.capture.settle);
    let reader = recognizer(&line);
    let read = reader.line("<990hp -7m 30mv> ").expect("it reads");
    assert_eq!(read.values["hp"], "990");
    assert_eq!(read.values["mana"], "-7");
    assert_eq!(read.values["move"], "30");
    assert!(reader.line("<990hp  7m 30mv> ").is_some());
    assert!(reader.line("You say '<990hp 7m 30mv> '").is_none());
    // A partial that ends where the line does is your prompt at once,
    // and one a read split before its end waits.
    assert!(reader.partial("<990hp 7m 30mv> ").is_some());
    assert!(reader.partial("<990hp 7m 30mv>").is_none());
    assert!(reader.partial("<990hp 7m 30m").is_none());
    // A line that ends on a number waits for its line end.
    let ends_on_a_number = generic("HP 100", &[]);
    assert!(!ends_on_a_number.capture.settle);
    assert!(recognizer(&ends_on_a_number).partial("HP 100").is_none());
}

#[test]
fn a_prompt_padded_to_one_width_matches_at_every_width() {
    // The game pads the prompt, so the spaces at its end shrink as the
    // number grows.
    let line = generic("<99hp>  ", &[]);
    assert_eq!(line.capture.lines[0], r"^<(?<hp>-?\d+)hp> +$");
    // It ends on spaces, not on a number, so it settles.
    assert!(line.capture.settle);
    let reader = recognizer(&line);
    for (prompt, hp) in [("<99hp>  ", "99"), ("<100hp> ", "100"), ("<9hp>   ", "9")] {
        assert_eq!(reader.line(prompt).expect(prompt).values["hp"], hp);
        assert!(reader.partial(prompt).is_some(), "{prompt:?}");
    }
    // A read split before the spaces still waits.
    assert!(reader.partial("<100hp>").is_none());
}

#[test]
fn names_you_give_rename_numbers_or_leave_them_out() {
    let line = generic("<100hp 50m 30mv> ", &["health", ""]);
    assert_eq!(
        line.capture.lines[0],
        r"^<(?<health>-?\d+)hp +-?\d+m +(?<move>-?\d+)mv> +$"
    );
    assert_eq!(
        (
            line.numbers[0].name.as_str(),
            line.numbers[0].label.as_str()
        ),
        ("health", "health")
    );
    assert_eq!(
        (
            line.numbers[1].name.as_str(),
            line.numbers[1].label.as_str()
        ),
        ("", "")
    );
    assert_eq!(line.numbers[1].suggested, "mana");
    assert_eq!(fills(&line.capture), ["health", "move"]);
    let read = recognizer(&line).line("<1hp 2m 3mv> ").expect("it reads");
    assert_eq!(
        read.values,
        BTreeMap::from([("health".into(), "1".into()), ("move".into(), "3".into())])
    );
    // A name no group can carry, or one given twice, goes by the
    // number of its group.
    let line = generic("1 2 3", &["9lives", "a", "a"]);
    assert_eq!(line.capture.lines[0], r"^(-?\d+) +(?<a>-?\d+) +(-?\d+)$");
    assert_eq!(
        line.capture.names,
        BTreeMap::from([("1".into(), "9lives".into()), ("3".into(), "a".into())])
    );
    let read = recognizer(&line).line("1 2 3").expect("it reads");
    assert_eq!(read.values["9lives"], "1");
    assert_eq!(read.values["a"], "2");
}

#[test]
fn a_line_with_no_number_read_only_recognizes_your_prompt() {
    let line = generic("> ", &[]);
    assert_eq!(line.capture.lines[0], "^> +$");
    assert!(line.capture.settle && line.numbers.is_empty());
    let read = recognizer(&line).line("> ").expect("it reads");
    assert!(read.values.is_empty());
    let line = generic("<100hp> ", &[""]);
    assert_eq!(line.capture.lines[0], r"^<-?\d+hp> +$");
    let leftover = &fills(&line.capture);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(recognizer(&line).line("<5hp> ").is_some());
}

#[test]
fn text_beyond_ascii_stays_whole() {
    let line = generic("♥100 ♦50 › ", &[]);
    assert_eq!(suggested("♥100 ♦50 › "), ["n1", "n2"]);
    assert_eq!(line.numbers[0].span, [3, 6]);
    let read = recognizer(&line).line("♥99 ♦5 › ").expect("it reads");
    assert_eq!(read.values["n1"], "99");
    assert_eq!(read.values["n2"], "5");
}
