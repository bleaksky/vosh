//! Tests of the bytes the stage writes around your prompt.

use std::collections::BTreeMap;

use super::marks::with_lift_end;
use super::output::{shows_anything, trailing_line_ends};
use super::*;
use crate::config::RegexCapture;

/// The capture the migration writes for James, unanchored.
const JAMES: &str =
    r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";
const PROMPT: &str = "[1020/1020hp 800/800mn 930/930mv]";
/// A prompt with no line end, read by a capture that settles.
const SETTLES: &str = r"^<(?<hp>\d+)hp> $";

fn stage(pattern: &str, settle: bool) -> Stage {
    let mut stage = Stage::default();
    stage.set_capture(&CaptureConfig::Regex(RegexCapture {
        lines: vec![pattern.to_string()],
        settle,
        ..RegexCapture::default()
    }));
    stage
}

fn with(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn read(stage: &Stage, text: &str, end: End) -> Block {
    stage
        .recognize(text.as_bytes(), text, end)
        .expect("the prompt")
}

#[test]
fn a_mark_is_a_private_osc_with_the_generation() {
    assert_eq!(mark(7), b"\x1b]7717;o;7\x07");
    assert_eq!(mark(12_345), b"\x1b]7717;o;12345\x07");
}

#[test]
fn a_line_prompt_draws_as_the_open_row_with_no_line_end() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    out.text(b"You are hungry.\r\n");
    let block = read(&stage, PROMPT, End::Line);
    assert_eq!(block.values["hp"], "1020");
    stage.draw(&mut out, block, None, b"", "DRAWN");
    assert_eq!(
        out.bytes,
        with(&[b"You are hungry.\r\n", &mark(1), b"DRAWN"])
    );
    assert_eq!(out.replace, None);
    assert_eq!(
        stage.open_row(),
        Some(&OpenRow {
            gen: 1,
            body: b"DRAWN".to_vec(),
            live: None,
            spans: Vec::new(),
            plain: String::new(),
        })
    );
    assert_eq!(
        stage.last_raw().map(Block::shown),
        Some(b"[1020/1020hp 800/800mn 930/930mv]\r\n".to_vec())
    );
}

#[test]
fn with_drawing_off_the_prompt_shows_as_sent() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let colored = "\x1b[37m[1020/1020hp 800/800mn 930/930mv]\x1b[0m";
    let block = stage
        .recognize(colored.as_bytes(), PROMPT, End::Line)
        .expect("the prompt");
    stage.show_as_sent(&mut out, block, None, b"", Some(colored.as_bytes()));
    assert_eq!(out.bytes, with(&[colored.as_bytes(), b"\r\n"]));
    assert_eq!(stage.open_row(), None);
    // A Prompts trigger that hides it leaves nothing.
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.show_as_sent(&mut out, block, None, b"", None);
    assert!(out.is_empty());
}

#[test]
fn echoes_land_where_the_prompt_was_before_the_drawn_prompt() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"low on mana\r\n", "DRAWN");
    assert_eq!(out.bytes, with(&[b"low on mana\r\n", &mark(1), b"DRAWN"]));
    assert_eq!(stage.open_row().map(|r| r.gen), Some(1));
}

#[test]
fn a_prompt_split_across_reads_replaces_the_painted_start() {
    let mut stage = stage(JAMES, false);
    // The first read ends partway through the prompt.
    let mut first = Output::new(false);
    let painted = stage.paint_partial(&mut first, b"[1020/1020hp 800", None);
    assert_eq!(painted, Some((1, 16)));
    assert_eq!(first.bytes, with(&[&mark(1), b"[1020/1020hp 800"]));
    // The next read completes it, and the drawn prompt replaces the
    // painted start in the same payload.
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut second, block, Some(1), b"", "DRAWN");
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 1,
            bytes: with(&[&mark(2), b"DRAWN"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &second.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(stage.open_row().map(|r| r.gen), Some(2));
}

#[test]
fn a_painted_start_after_other_output_draws_on_a_new_row() {
    let mut stage = stage(JAMES, false);
    let mut first = Output::new(false);
    let painted = stage.paint_partial(&mut first, b"[1020", None);
    assert_eq!(painted, Some((1, 5)));

    // A GMCP handler's echo came first in the next read.
    let mut second = Output::new(false);
    second.text(b"\r\nThe moon rises.\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut second, block, Some(1), b"", "DRAWN");
    assert_eq!(second.replace, None);
    assert_eq!(
        second.bytes,
        with(&[b"\r\nThe moon rises.\r\n", &mark(2), b"DRAWN"])
    );

    // Text that leaves the cursor mid row gets a line end first.
    let mut third = Output::new(false);
    third.text(b"mid row");
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut third, block, Some(9), b"", "DRAWN");
    assert_eq!(third.bytes, with(&[b"mid row\r\n", &mark(3), b"DRAWN"]));
}

#[test]
fn a_line_completing_a_painted_partial_replaces_it() {
    let mut stage = stage(JAMES, false);
    let mut first = Output::new(false);
    let _ = stage.paint_partial(&mut first, b"You are hun", None);
    let mut second = Output::new(false);
    stage.line(
        &mut second,
        b"You are hungry.",
        "You are hungry.",
        Some(1),
        b"You are hungry.\r\n",
    );
    stage.line(&mut second, b"next", "next", None, b"next\r\n");
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 1,
            bytes: b"You are hungry.\r\n".to_vec(),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    assert_eq!(second.bytes, b"next\r\n");

    // A trigger that hides the completed line erases the painted
    // start, and a renderer that wrote after it drops the erase.
    let mut third = Output::new(false);
    let _ = stage.paint_partial(&mut third, b"spam", None);
    let mut fourth = Output::new(false);
    stage.line(&mut fourth, b"spam spam", "spam spam", Some(2), b"");
    assert_eq!(
        fourth.replace,
        Some(Replace {
            gen: 2,
            bytes: Vec::new(),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
}

#[test]
fn a_growing_partial_is_painted_again_whole() {
    let mut stage = stage(JAMES, false);
    let mut first = Output::new(false);
    let painted = stage.paint_partial(&mut first, b"<10", None);
    let mut second = Output::new(false);
    let painted = stage.paint_partial(&mut second, b"<10hp> ", painted);
    assert_eq!(painted, Some((2, 7)));
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 1,
            bytes: with(&[&mark(2), b"<10hp> "]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    // A read that adds nothing to it writes nothing.
    let mut third = Output::new(false);
    assert_eq!(
        stage.paint_partial(&mut third, b"<10hp> ", painted),
        painted
    );
    assert!(third.is_empty());
    // An empty partial paints nothing.
    assert_eq!(stage.paint_partial(&mut third, b"", None), None);
    assert!(third.is_empty());
}

#[test]
fn a_partial_that_settles_is_the_prompt_at_once() {
    let mut stage = stage(SETTLES, true);
    assert!(stage.recognize(b"<10hp>", "<10hp>", End::Settled).is_none());
    let block = read(&stage, "<10hp> ", End::Settled);
    let mut out = Output::new(false);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN"]));
    // Shown as sent, a settled prompt keeps the cursor after it.
    let mut out = Output::new(false);
    let block = read(&stage, "<10hp> ", End::Settled);
    stage.show_as_sent(&mut out, block, None, b"", Some(b"<10hp> "));
    assert_eq!(out.bytes, b"<10hp> ");
    // A capture that waits never reads a partial.
    let waits = self::stage(JAMES, false);
    assert!(waits
        .recognize(PROMPT.as_bytes(), PROMPT, End::Settled)
        .is_none());
}

#[test]
fn a_prompt_whole_before_its_line_end_draws_the_line_end_after_it() {
    let mut stage = stage(SETTLES, true);
    let block = read(&stage, "<10hp> ", End::Line);
    assert_eq!(block.final_line().end, End::SettledLine);
    let mut out = Output::new(false);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN\r\n"]));
    // The row stays open with its line end, and a repaint keeps it.
    assert_eq!(
        stage.open_row(),
        Some(&OpenRow {
            gen: 1,
            body: b"DRAWN\r\n".to_vec(),
            live: None,
            spans: Vec::new(),
            plain: String::new(),
        })
    );
    let mut new = Output::new(false);
    stage.repaint(&mut new, Some("NEW"));
    assert_eq!(
        new.replace,
        Some(Replace {
            gen: 1,
            bytes: with(&[&mark(2), b"NEW\r\n"]),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    let mut off = Output::new(false);
    stage.repaint(&mut off, None);
    assert_eq!(
        off.replace.map(|r| r.bytes),
        Some(with(&[&mark(3), b"<10hp> \r\n"]))
    );
    // A capture that waits for its line end reads the line end as
    // part of the prompt, so the drawn prompt keeps the cursor.
    let mut waits = self::stage(JAMES, false);
    let block = read(&waits, PROMPT, End::Line);
    assert_eq!(block.final_line().end, End::Line);
    let mut out = Output::new(false);
    waits.draw(&mut out, block, None, b"", "DRAWN");
    assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN"]));
}

#[test]
fn a_ga_after_a_prompt_that_settles_leaves_the_cursor_after_it() {
    let mut stage = stage(SETTLES, true);
    let block = read(&stage, "<10hp> ", End::Marker);
    assert_eq!(block.final_line().end, End::Settled);
    let mut out = Output::new(false);
    stage.show_as_sent(&mut out, block, None, b"", Some(b"<10hp> "));
    assert_eq!(out.bytes, b"<10hp> ");
}

#[test]
fn a_ga_ends_a_prompt_in_the_same_read_or_the_next() {
    let mut stage = stage(JAMES, false);
    // The same read: the partial never painted, so nothing flashes.
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Marker);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    assert_eq!(out.bytes, with(&[&mark(1), b"DRAWN"]));

    // The next read: the end of the first painted it, and the GA
    // replaces it.
    let mut first = Output::new(false);
    let painted = stage.paint_partial(&mut first, PROMPT.as_bytes(), None);
    assert_eq!(painted, Some((2, PROMPT.len())));
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Marker);
    stage.draw(&mut second, block, Some(2), b"", "DRAWN");
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 2,
            bytes: with(&[&mark(3), b"DRAWN"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );

    // Drawing off, a GA ends the row after the prompt.
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Marker);
    stage.show_as_sent(&mut out, block, None, b"", Some(PROMPT.as_bytes()));
    assert_eq!(out.bytes, with(&[PROMPT.as_bytes(), b"\r\n"]));
}

#[test]
fn a_ga_on_a_partial_vosh_does_not_read_ends_the_row() {
    let mut stage = stage(JAMES, false);
    // Same read, never painted.
    let mut out = Output::new(false);
    stage.end_partial(&mut out, b"[Hit Return]", None, b"", Some(b"[Hit Return]"));
    assert_eq!(out.bytes, b"[Hit Return]\r\n");
    // Painted as it is, so only the row ends.
    let mut out = Output::new(false);
    stage.end_partial(&mut out, b"> ", Some((4, 2)), b"", Some(b"> "));
    assert_eq!(out.bytes, b"\r\n");
    assert_eq!(out.replace, None);
    // It grew in the read the GA came in, so the painted start is
    // replaced by the whole of it.
    let mut out = Output::new(false);
    stage.end_partial(
        &mut out,
        b"<100hp 50m 30mv> ",
        Some((4, 9)),
        b"",
        Some(b"<100hp 50m 30mv> "),
    );
    assert_eq!(
        out.replace,
        Some(Replace {
            gen: 4,
            bytes: b"<100hp 50m 30mv> \r\n".to_vec(),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &out.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    // A Prompts trigger changed it, so it replaces the painted one.
    let mut out = Output::new(false);
    stage.end_partial(
        &mut out,
        b"> ",
        Some((4, 2)),
        b"",
        Some(b"\x1b[31m> \x1b[0m"),
    );
    assert_eq!(
        out.replace,
        Some(Replace {
            gen: 4,
            bytes: b"\x1b[31m> \x1b[0m\r\n".to_vec(),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    // A trigger hid it: the painted one is erased, and an unpainted
    // one writes nothing.
    let mut out = Output::new(false);
    stage.end_partial(&mut out, b"> ", Some((4, 2)), b"", None);
    assert_eq!(
        out.replace,
        Some(Replace {
            gen: 4,
            bytes: Vec::new(),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    let mut out = Output::new(false);
    stage.end_partial(&mut out, b"> ", None, b"", None);
    assert!(out.is_empty());
}

#[test]
fn draw_off_repaints_the_open_row_as_the_game_sent_it() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");

    let mut off = Output::new(false);
    stage.repaint(&mut off, None);
    assert_eq!(
        off.replace,
        Some(Replace {
            gen: 1,
            bytes: with(&[&mark(2), PROMPT.as_bytes(), b"\r\n"]),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &off.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");

    // Drawing back on paints the design again over the same row.
    let mut on = Output::new(false);
    stage.repaint(&mut on, Some("DRAWN"));
    assert_eq!(
        on.replace,
        Some(Replace {
            gen: 2,
            bytes: with(&[&mark(3), b"DRAWN"]),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    // A repaint that changes nothing writes nothing.
    let mut same = Output::new(false);
    stage.repaint(&mut same, Some("DRAWN"));
    assert!(same.is_empty());
    let mut new = Output::new(false);
    stage.repaint(&mut new, Some("NEW DESIGN"));
    assert_eq!(new.replace.map(|r| r.gen), Some(3));
}

#[test]
fn the_open_row_closes_on_output_a_send_and_other_output() {
    let block_of = |stage: &Stage| read(stage, PROMPT, End::Line);

    // Output after it in the same read.
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
    out.text(b"You flee!\r\n");
    stage.finish(&mut out);
    let mut later = Output::new(false);
    stage.repaint(&mut later, None);
    assert!(later.is_empty());
    assert_eq!(stage.open_row(), None);

    // A send, a local write or a window size change.
    let mut out = Output::new(false);
    stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
    stage.close();
    let mut later = Output::new(false);
    stage.repaint(&mut later, None);
    assert!(later.is_empty());

    // Output from elsewhere, such as a slash command's echo.
    let mut out = Output::new(false);
    stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
    let mut later = Output::new(true);
    stage.repaint(&mut later, None);
    assert!(later.is_empty());

    // A partial painted after it.
    let mut out = Output::new(false);
    stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
    let _ = stage.paint_partial(&mut out, b"more", None);
    assert_eq!(stage.open_row(), None);

    // A hidden line writes nothing, so the row stays open.
    let mut out = Output::new(false);
    stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
    stage.line(&mut out, b"spam", "spam", None, b"");
    stage.finish(&mut out);
    assert!(stage.open_row().is_some());

    // A prompt drawn after other output in the same read stays open.
    let mut out = Output::new(true);
    out.text(b"text\r\n");
    stage.draw(&mut out, block_of(&stage), None, b"", "DRAWN");
    stage.finish(&mut out);
    assert!(stage.open_row().is_some());
}

#[test]
fn a_local_write_closes_only_a_row_drawn_before_the_text_landed() {
    let block_of = |stage: &Stage| read(stage, PROMPT, End::Line);
    let mut stage = stage(JAMES, false);
    let mut first = Output::new(false);
    stage.draw(&mut first, block_of(&stage), None, b"", "FIRST");
    stage.finish(&mut first);

    // Your echo landed after the first prompt, but the session hears
    // of it only after it sent the next one, which follows the echo.
    stage.close();
    let mut next = Output::new(false);
    next.text(b"reply\r\n");
    stage.draw(&mut next, block_of(&stage), None, b"", "NEXT");
    stage.finish(&mut next);
    assert!(stage.wrote_after(first.id()));
    stage.local_write(first.id());
    assert_eq!(stage.open_row().map(|r| r.gen), Some(2));

    // A repaint keeps the output the row came in, since a renderer
    // drops a repaint of a row text landed after.
    let mut repaint = Output::new(false);
    stage.repaint(&mut repaint, Some("EDITED"));
    assert_eq!(repaint.replace.as_ref().map(|r| r.gen), Some(2));
    assert!(stage.wrote_after(next.id()));
    stage.local_write(first.id());
    assert!(stage.open_row().is_some());
    stage.local_write(next.id());
    assert_eq!(stage.open_row(), None);

    // Text that landed after the newest output closes the row.
    let mut last = Output::new(false);
    stage.draw(&mut last, block_of(&stage), None, b"", "LAST");
    stage.finish(&mut last);
    assert!(!stage.wrote_after(last.id()));
    stage.local_write(last.id());
    assert_eq!(stage.open_row(), None);

    // An output with nothing to write never reaches a renderer.
    let mut nothing = Output::new(false);
    stage.finish(&mut nothing);
    assert!(!stage.wrote_after(last.id()));
}

/// A span of piece `piece` on the first row, `width` cells from
/// `col`.
fn span_at(piece: usize, col: usize, width: usize) -> Span {
    Span {
        piece,
        row: 0,
        col,
        width,
        fg: crate::render::SpanColor::Default,
        bg: crate::render::SpanColor::Default,
        bold: false,
        italic: false,
        underline: false,
        push: false,
        look: crate::render::SgrState::default(),
    }
}

/// `shown` with its pieces, as the session hands a render over.
fn drawn_view<'a>(shown: &'a str, spans: &'a [Span]) -> View<'a> {
    View {
        shown: Some(shown),
        spans,
        plain: shown,
        ..View::default()
    }
}

#[test]
fn the_open_row_keeps_the_pieces_of_the_render_that_drew_it() {
    let one = [span_at(0, 0, 5)];
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", drawn_view("DRAWN", &one));
    let open = stage.open_row().expect("the row");
    assert_eq!(
        (open.spans.clone(), open.plain.as_str()),
        (one.to_vec(), "DRAWN")
    );

    // A repaint that draws the same bytes keeps them, and takes the
    // latest render's pieces, since an edit can number them anew.
    let renumbered = [span_at(0, 0, 2), span_at(1, 2, 3)];
    let mut same = Output::new(false);
    stage.repaint_view(&mut same, drawn_view("DRAWN", &renumbered));
    assert!(same.is_empty());
    let open = stage.open_row().expect("the row");
    assert_eq!(open.spans, renumbered);

    // Another design brings its own.
    let new = [span_at(0, 0, 3)];
    let mut out = Output::new(false);
    stage.repaint_view(&mut out, drawn_view("NEW", &new));
    let open = stage.open_row().expect("the row");
    assert_eq!(
        (open.spans.clone(), open.plain.as_str()),
        (new.to_vec(), "NEW")
    );

    // The game's own line has none, drawing off or under the card.
    let mut off = Output::new(false);
    stage.repaint_view(&mut off, View::live(None));
    let open = stage.open_row().expect("the row");
    assert!(open.spans.is_empty() && open.plain.is_empty());
    let mut raw = Output::new(false);
    stage.repaint_view(
        &mut raw,
        View {
            live: Some("NEW"),
            spans: &new,
            plain: "NEW",
            ..View::default()
        },
    );
    let open = stage.open_row().expect("the row");
    assert!(open.spans.is_empty() && open.plain.is_empty());

    // A prompt that comes back from the band brings them too.
    let mut stage = self::stage(JAMES, false);
    stage.set_show(PromptShow::Pinned);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_view(&mut out, block, None, b"", drawn_view("DRAWN", &one));
    stage.finish(&mut out);
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint_view(&mut back, drawn_view("DRAWN", &one));
    let open = stage.open_row().expect("the row");
    assert_eq!(open.spans, one);
}

#[test]
fn a_reset_forgets_the_session_and_keeps_counting() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.record(None, 1, true, true);
    assert!(stage.gag_without_reader("capture"));
    stage.reset();
    assert!(stage.has_recognizer());
    assert_eq!(stage.open_row(), None);
    assert_eq!(stage.last_raw(), None);
    assert_eq!(stage.ring().count(), 0);
    assert_eq!(stage.line_trigger_notice(), None);
    assert!(stage.gag_without_reader("capture"));
    assert_eq!(stage.next_gen(), 2);
}

#[test]
fn the_ring_records_one_entry_per_send_and_ga() {
    let mut stage = stage(JAMES, false);
    stage.record(None, 1, true, true);
    assert_eq!(stage.ring().count(), 0, "nothing came in yet");

    // A drawn prompt is recorded as the game sent it, before the gag.
    let mut out = Output::new(false);
    let colored = "\x1b[37m[1020/1020hp 800/800mn 930/930mv]";
    let block = stage
        .recognize(colored.as_bytes(), PROMPT, End::Line)
        .expect("the prompt");
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.record(None, 10, true, true);
    // Nothing new came in, so a second send records nothing.
    stage.record(None, 11, true, true);

    // A line that is not a prompt, then a partial at a send.
    stage.line(
        &mut out,
        b"You are hungry.",
        "You are hungry.",
        None,
        b"You are hungry.\r\n",
    );
    stage.record(Some((b"<10hp> ", "<10hp> ")), 20, false, true);
    // A complete line, with no prompt and no partial.
    stage.line(&mut out, b"", "", None, b"\r\n");
    stage.line(&mut out, b"Healer> ", "Healer> ", None, b"Healer> \r\n");
    stage.line(&mut out, b"  ", "  ", None, b"  \r\n");
    stage.record(Some((b"", "")), 30, false, false);

    let ring: Vec<&Candidate> = stage.ring().collect();
    assert_eq!(
        ring[0],
        &Candidate {
            id: 1,
            raw: colored.as_bytes().to_vec(),
            plain: PROMPT.to_string(),
            at_ms: 10,
            recognized: true,
            draw: true,
            capture: true,
        }
    );
    assert_eq!(
        ring[1],
        &Candidate {
            id: 2,
            raw: b"<10hp> ".to_vec(),
            plain: "<10hp> ".to_string(),
            at_ms: 20,
            recognized: false,
            draw: false,
            capture: true,
        }
    );
    assert_eq!(
        ring[2],
        &Candidate {
            id: 3,
            raw: b"Healer> ".to_vec(),
            plain: "Healer> ".to_string(),
            at_ms: 30,
            recognized: false,
            draw: false,
            capture: false,
        }
    );
    assert_eq!(ring.len(), 3);
    assert_eq!(stage.candidate(2).map(|c| c.at_ms), Some(20));
    assert_eq!(stage.candidate(4), None);

    // A new connection starts the ring over and keeps counting, so
    // an id from before never names a new entry.
    stage.reset();
    stage.line(&mut out, b"> ", "> ", None, b"> \r\n");
    stage.record(None, 40, false, true);
    assert_eq!(stage.ring().map(|c| c.id).collect::<Vec<_>>(), [4]);
    assert_eq!(stage.candidate(1), None);
}

#[test]
fn the_ring_keeps_the_newest_thirty_two() {
    let mut stage = Stage::default();
    let mut out = Output::new(false);
    for i in 0..40_i64 {
        let text = format!("line {i}");
        stage.line(&mut out, text.as_bytes(), &text, None, b"");
        stage.record(None, i, false, false);
    }
    let ring: Vec<i64> = stage.ring().map(|c| c.at_ms).collect();
    assert_eq!(ring.len(), RING);
    assert_eq!(ring.first(), Some(&8));
    assert_eq!(ring.last(), Some(&39));
}

#[test]
fn a_gag_with_no_reader_is_told_once_per_trigger() {
    let mut stage = Stage::default();
    assert!(!stage.has_recognizer());
    assert!(stage.gag_without_reader("prompt-capture"));
    assert!(!stage.gag_without_reader("prompt-capture"));
    assert!(stage.gag_without_reader("my-capture"));
    assert_eq!(
        stage.gags_without_reader().collect::<Vec<_>>(),
        ["my-capture", "prompt-capture"]
    );
    // Once the profile reads your prompt, no trigger hides it with
    // nothing drawn, so the list starts over.
    stage.set_capture(&CaptureConfig::Regex(RegexCapture {
        lines: vec![JAMES.to_string()],
        ..RegexCapture::default()
    }));
    assert_eq!(stage.gags_without_reader().count(), 0);
    // A profile that reads none again hears of each trigger anew.
    stage.set_capture(&CaptureConfig::None);
    assert!(stage.gag_without_reader("my-capture"));
    // Another profile starts its own list.
    stage.forget_gags_without_reader();
    assert_eq!(stage.gags_without_reader().count(), 0);
}

#[test]
fn line_triggers_that_matched_a_prompt_are_named_after_one_was_read() {
    let mut stage = stage(JAMES, false);
    stage.line_triggers_matched(["hp-watch"]);
    assert_eq!(stage.line_trigger_notice(), None, "no prompt read yet");
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.line_triggers_matched(["bracket", "hp-watch"]);
    assert_eq!(
        stage.line_trigger_notice(),
        Some(vec!["bracket".to_string(), "hp-watch".to_string()])
    );
}

#[test]
fn a_block_keeps_its_raw_text_for_the_raw_piece() {
    let stage = stage(JAMES, false);
    let colored = "\x1b[37m[1020/1020hp 800/800mn 930/930mv]\x1b[0m";
    let block = stage
        .recognize(colored.as_bytes(), PROMPT, End::Line)
        .expect("the prompt");
    assert_eq!(block.raw_text(), colored);
    assert_eq!(block.final_line().plain, PROMPT);
}

/// A stage that reads JAMES and pins your prompt.
fn pinned_stage() -> Stage {
    let mut stage = stage(JAMES, false);
    stage.set_show(PromptShow::Pinned);
    stage
}

/// Pin the prompt as the session does with drawing on.
fn pin_prompt(stage: &mut Stage, out: &mut Output) {
    let block = read(stage, PROMPT, End::Line);
    stage.pin_drawn(out, block, None, b"", "DRAWN");
}

#[test]
fn the_line_ends_a_text_ends_on_are_found_after_its_last_visible_character() {
    assert_eq!(trailing_line_ends(b"room\r\n\r\n"), 4);
    assert_eq!(trailing_line_ends(b"room\x1b[0m\r\n\x1b[0m\r\n"), 4);
    assert_eq!(trailing_line_ends(b"room"), 4);
    assert_eq!(
        trailing_line_ends(b"room\x1b[0m"),
        8,
        "no line end, no hold"
    );
    assert_eq!(trailing_line_ends(b"\r\n"), 0);
    assert_eq!(trailing_line_ends(b""), 0);
    // A mark stops the run, so a region keeps its start.
    let mut marked = b"a\r\n".to_vec();
    marked.extend(mark(3));
    marked.extend_from_slice(b"\r\n");
    assert_eq!(trailing_line_ends(&marked), marked.len() - 2);
    assert!(shows_anything(b"a"));
    assert!(!shows_anything(b"\r\n \x1b[0m\x1b]7717;o;4\x07"));
}

#[test]
fn a_pinned_prompt_leaves_the_text_and_holds_the_line_ends_before_it() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"The Bank of Aabahran\r\n[Exits: south]\r\n");
    stage.line(&mut out, b"", "", None, b"\r\n");
    pin_prompt(&mut stage, &mut out);
    assert_eq!(out.bytes, b"The Bank of Aabahran\r\n[Exits: south]");
    assert_eq!(out.hold, b"\r\n\r\n");
    assert_eq!(out.pin.as_deref(), Some(&b"DRAWN"[..]));
    assert_eq!(out.replace, None);
    assert_eq!(stage.open_row(), None);
    assert_eq!(stage.pinned(), Some(&b"DRAWN"[..]));
    assert!(stage.swallows());
    stage.finish(&mut out);

    // The next unasked text starts with an empty line, which writes
    // nothing, then lands where the prompt's row was.
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    assert!(next.bytes.is_empty() && next.hold.is_empty());
    stage.line(
        &mut next,
        b"Joral tells you 'hi'",
        "Joral tells you 'hi'",
        None,
        b"Joral tells you 'hi'\r\n",
    );
    stage.line(&mut next, b"", "", None, b"\r\n");
    pin_prompt(&mut stage, &mut next);
    assert_eq!(next.bytes, b"Joral tells you 'hi'");
    assert_eq!(next.hold, b"\r\n\r\n");
    // Once text lands, the next empty line writes again.
    let mut more = Output::new(false);
    stage.line(&mut more, b"", "", None, b"\r\n");
    assert!(more.bytes.is_empty(), "the second pin armed it again");
}

#[test]
fn two_pins_in_one_read_keep_the_hold_at_the_end_of_the_output() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    assert_eq!(out.hold, b"\r\n\r\n");
    // The next pulse in the same read.
    stage.line(&mut out, b"", "", None, b"\r\n");
    stage.line(&mut out, b"tell", "tell", None, b"tell\r\n");
    assert_eq!(
        out.bytes, b"room\r\n\r\ntell\r\n",
        "the hold went back first"
    );
    let leftover = &out.hold;
    assert!(leftover.is_empty(), "{leftover:?}");
    stage.line(&mut out, b"", "", None, b"\r\n");
    pin_prompt(&mut stage, &mut out);
    assert_eq!(out.bytes, b"room\r\n\r\ntell");
    assert_eq!(out.hold, b"\r\n\r\n");
    // A region painted after a pin takes the hold back too.
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    let _ = stage.paint_partial(&mut out, b"<10", None);
    let leftover = &out.hold;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(out.bytes.starts_with(b"room\r\n\r\n\x1b]7717;o;"));
}

#[test]
fn the_end_of_a_read_keeps_the_swallow_however_often_it_is_told() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.end_read(&mut out);
    stage.finish(&mut out);
    stage.finish(&mut out);
    assert!(stage.swallows());
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    let leftover = &next.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    // Text in a later output ends it, whoever wrote it.
    next.text(b"echo\r\n");
    stage.finish(&mut next);
    assert!(!stage.swallows());
}

#[test]
fn a_pulse_of_hidden_lines_moves_nothing() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    stage.line(&mut next, b"spam", "spam", None, b"");
    stage.line(&mut next, b"", "", None, b"\r\n");
    pin_prompt(&mut stage, &mut next);
    let leftover = &next.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &next.hold;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(next.pin.as_deref(), Some(&b"DRAWN"[..]));
    assert!(!next.is_empty(), "the band still changes");
}

#[test]
fn enter_on_an_empty_line_moves_nothing_and_updates_the_band() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    // A send leaves it armed.
    stage.close();
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(&mut next, block, None, b"", "NEW");
    assert!(next.bytes.is_empty() && next.hold.is_empty());
    assert_eq!(next.pin.as_deref(), Some(&b"NEW"[..]));
    assert!(!next.is_empty());
}

#[test]
fn a_local_write_that_landed_before_a_pin_leaves_its_row_open() {
    let mut stage = pinned_stage();
    let mut first = Output::new(false);
    pin_prompt(&mut stage, &mut first);
    stage.finish(&mut first);
    let mut next = Output::new(false);
    next.text(b"reply\r\n");
    pin_prompt(&mut stage, &mut next);
    stage.finish(&mut next);
    stage.local_write(first.id());
    assert!(stage.swallows());
    stage.local_write(next.id());
    assert!(!stage.swallows());
}

#[test]
fn a_local_write_and_other_output_end_the_swallow() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    stage.local_write(out.id());
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    assert_eq!(next.bytes, b"\r\n");

    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    let mut next = Output::new(true);
    stage.line(&mut next, b"", "", None, b"\r\n");
    assert_eq!(next.bytes, b"\r\n");

    // Text a script echoed in the same read ends it too.
    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    out.text(b"echo\r\n");
    stage.line(&mut out, b"", "", None, b"\r\n");
    assert!(out.bytes.ends_with(b"echo\r\n\r\n"));

    // Output from elsewhere that came before the read the prompt
    // pinned in came before the prompt, so it ends nothing.
    let mut out = Output::new(true);
    out.text(b"tell\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    assert!(stage.swallows());
    assert_eq!(out.pin_row, Some(true));
}

#[test]
fn a_prompt_that_took_its_line_end_arms_nothing() {
    let mut stage = stage(SETTLES, true);
    stage.set_show(PromptShow::Pinned);
    let block = read(&stage, "<10hp> ", End::Line);
    assert_eq!(block.final_line().end, End::SettledLine);
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    stage.pin_drawn(&mut out, block, None, b"", "DRAWN");
    assert!(!stage.swallows());
    assert_eq!(
        out.pin.as_deref(),
        Some(&b"DRAWN"[..]),
        "no line end on the band"
    );
    stage.line(&mut out, b"arrives", "arrives", None, b"arrives\r\n");
    assert_eq!(out.bytes, b"room\r\n\r\narrives\r\n");
}

#[test]
fn a_painted_start_of_a_pinned_prompt_is_erased_unless_closed() {
    let mut stage = pinned_stage();
    let mut first = Output::new(false);
    let painted = stage.paint_partial(&mut first, b"[1020/1020hp 800", None);
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(&mut second, block, painted.map(|(g, _)| g), b"", "DRAWN");
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 1,
            bytes: Vec::new(),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &second.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    // Echoes a Prompts trigger wrote take the painted region's place,
    // line end and all, since a replace is written whole.
    let mut third = Output::new(false);
    let painted = stage.paint_partial(&mut third, b"[1020", None);
    let mut fourth = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(
        &mut fourth,
        block,
        painted.map(|(g, _)| g),
        b"low on mana\r\n",
        "DRAWN",
    );
    assert_eq!(
        fourth.replace,
        Some(Replace {
            gen: 2,
            bytes: b"low on mana\r\n".to_vec(),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    // After other output the painted start is closed, so it stays.
    let mut fifth = Output::new(false);
    let painted = stage.paint_partial(&mut fifth, b"[1020", None);
    let mut sixth = Output::new(false);
    sixth.text(b"The moon rises.\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(&mut sixth, block, painted.map(|(g, _)| g), b"", "DRAWN");
    assert_eq!(sixth.replace, None);
    assert_eq!(sixth.bytes, b"The moon rises.");
    assert_eq!(sixth.hold, b"\r\n");
}

#[test]
fn a_pinned_prompt_that_spans_lines_puts_every_line_on_the_band() {
    let stage = pinned_stage();
    let block = Block {
        lines: vec![
            BlockLine {
                raw: b"Tester: [===|---]".to_vec(),
                plain: "Tester: [===|---]".into(),
                end: End::Line,
            },
            BlockLine {
                raw: PROMPT.as_bytes().to_vec(),
                plain: PROMPT.into(),
                end: End::Line,
            },
        ],
        replaced: vec![1],
        values: BTreeMap::new(),
        afk: false,
        groups: Vec::new(),
    };
    let mut stage = stage;
    let mut out = Output::new(false);
    out.text(b"A guard has quite a few wounds.\r\n\r\n");
    stage.pin_drawn(&mut out, block.clone(), None, b"", "DRAWN");
    assert_eq!(out.pin.as_deref(), Some(&b"Tester: [===|---]\r\nDRAWN"[..]));
    assert_eq!(out.bytes, b"A guard has quite a few wounds.");
    // A design that reads the tank line takes it over.
    let over = Block {
        replaced: vec![0, 1],
        ..block.clone()
    };
    let mut out = Output::new(false);
    stage.pin_drawn(&mut out, over, None, b"", "Tank 75%\r\nDRAWN");
    assert_eq!(out.pin.as_deref(), Some(&b"Tank 75%\r\nDRAWN"[..]));
    // Drawing off, the band shows the lines as sent, and a trigger
    // that hid the last one leaves the lines above it.
    let mut out = Output::new(false);
    stage.pin_shown(
        &mut out,
        block.clone(),
        None,
        b"",
        Some(b"\x1b[31m[shown]\x1b[0m"),
    );
    assert_eq!(
        out.pin.as_deref(),
        Some(&b"Tester: [===|---]\r\n\x1b[31m[shown]\x1b[0m"[..])
    );
    let mut out = Output::new(false);
    stage.pin_shown(&mut out, block, None, b"", None);
    assert_eq!(out.pin.as_deref(), Some(&b"Tester: [===|---]"[..]));
}

#[test]
fn a_repaint_while_pinned_changes_only_the_band() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    let mut repaint = Output::new(false);
    stage.repaint(&mut repaint, Some("NEW"));
    assert_eq!(repaint.pin.as_deref(), Some(&b"NEW"[..]));
    assert!(repaint.bytes.is_empty() && repaint.replace.is_none() && repaint.hold.is_empty());
    let mut same = Output::new(false);
    stage.repaint(&mut same, Some("NEW"));
    assert!(same.is_empty());
    let mut off = Output::new(false);
    stage.repaint(&mut off, None);
    assert_eq!(off.pin.as_deref(), Some(PROMPT.as_bytes()));
    // Your echo after it changes nothing about that.
    stage.local_write(off.id());
    let mut later = Output::new(false);
    stage.repaint(&mut later, Some("LATER"));
    assert_eq!(later.pin.as_deref(), Some(&b"LATER"[..]));
}

#[test]
fn choosing_pinned_moves_the_open_row_to_the_band() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.finish(&mut out);
    stage.set_show(PromptShow::Pinned);
    let mut moved = Output::new(false);
    stage.repaint(&mut moved, Some("DRAWN"));
    assert_eq!(
        moved.replace,
        Some(Replace {
            gen: 1,
            bytes: Vec::new(),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    assert_eq!(moved.pin.as_deref(), Some(&b"DRAWN"[..]));
    assert!(stage.swallows());
    assert_eq!(stage.open_row(), None);
    // With the row closed, the next prompt goes to the band.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.close();
    stage.set_show(PromptShow::Pinned);
    let mut moved = Output::new(false);
    stage.repaint(&mut moved, Some("DRAWN"));
    assert!(moved.is_empty());
}

#[test]
fn leaving_pinned_brings_the_prompt_back_only_while_its_row_would_still_be_last() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint(&mut back, Some("DRAWN"));
    assert_eq!(back.pin.as_deref(), Some(&b""[..]), "the band empties");
    assert_eq!(back.bytes, with(&[&mark(1), b"DRAWN"]));
    assert_eq!(stage.open_row().map(|r| r.gen), Some(1));
    assert!(!stage.swallows());

    // After your echo the prompt stays off the text.
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    stage.local_write(out.id());
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint(&mut back, Some("DRAWN"));
    assert_eq!(back.pin.as_deref(), Some(&b""[..]));
    let leftover = &back.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(stage.open_row(), None);
}

#[test]
fn the_line_end_that_would_end_a_pinned_row_is_found_past_escapes() {
    let cut = |bytes: &[u8]| {
        let (rest, closed) = close_pin_row(bytes);
        (rest.into_owned(), closed)
    };
    assert_eq!(cut(b"\r\nTICK\r\n"), (b"TICK\r\n".to_vec(), true));
    assert_eq!(cut(b"\n"), (Vec::new(), true));
    // Colors before it stay, since they write nothing.
    assert_eq!(cut(b"\x1b[33m\r\nTICK"), (b"\x1b[33mTICK".to_vec(), true));
    // Text that shows first fills the row, so nothing goes.
    assert_eq!(cut(b"look\r\n"), (b"look\r\n".to_vec(), true));
    assert_eq!(cut(b" look"), (b" look".to_vec(), true));
    // Escapes alone leave the row open.
    assert_eq!(cut(b"\x1b[0m"), (b"\x1b[0m".to_vec(), false));
    assert_eq!(cut(b""), (Vec::new(), false));
}

#[test]
fn a_framed_echo_after_a_pinned_prompt_takes_the_prompt_row() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    // A script echo in the same read, framed with a line end first to
    // end the prompt's row. That row is not in the text, so the line
    // end writes nothing and the echo takes the row.
    out.text(b"\r\nThe moon rises.\r\n");
    assert_eq!(out.bytes, b"room\r\n\r\nThe moon rises.\r\n");
    let leftover = &out.hold;
    assert!(leftover.is_empty(), "{leftover:?}");
    // Only the first one goes.
    out.text(b"\r\nThe sun sets.\r\n");
    assert_eq!(
        out.bytes,
        b"room\r\n\r\nThe moon rises.\r\n\r\nThe sun sets.\r\n"
    );
    // Swallowed empty lines leave the row open for the echo after them.
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.line(&mut out, b"", "", None, b"\r\n");
    out.text(b"\r\nThe moon rises.\r\n");
    assert_eq!(out.bytes, b"room\r\n\r\nThe moon rises.\r\n");
}

#[test]
fn each_pinned_output_says_whether_the_prompt_row_is_open() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    assert_eq!(out.pin_row, Some(true));
    // Text that lands closes it.
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    stage.line(&mut next, b"tell", "tell", None, b"tell\r\n");
    stage.finish(&mut next);
    assert_eq!(next.pin_row, Some(false));
    // A prompt that took its line end leaves no row open.
    let mut settles = stage_settling_pinned();
    let block = read(&settles, "<10hp> ", End::Line);
    let mut out = Output::new(false);
    settles.pin_drawn(&mut out, block, None, b"", "DRAWN");
    settles.finish(&mut out);
    assert_eq!(out.pin_row, Some(false));
    // Leaving Pinned closes it with the band.
    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint(&mut back, Some("DRAWN"));
    assert_eq!(back.pin_row, Some(false));
    // In the text no output says anything about it.
    let mut text = self::stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&text, PROMPT, End::Line);
    text.draw(&mut out, block, None, b"", "DRAWN");
    text.finish(&mut out);
    assert_eq!(out.pin_row, None);
    let mut again = Output::new(false);
    text.repaint(&mut again, Some("NEW"));
    assert_eq!(again.pin_row, None);
}

#[test]
fn a_repaint_of_the_band_alone_says_nothing_about_the_row() {
    // A renderer can close the row with your echo before the session
    // hears of it, so a repaint that only changes the band leaves the
    // row as each renderer has it.
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    assert_eq!(out.pin_row, Some(true));
    let mut band = Output::new(false);
    stage.repaint(&mut band, Some("NEW"));
    assert!(band.pin.is_some(), "the band repaints");
    assert!(band.bytes.is_empty() && band.replace.is_none());
    assert_eq!(band.pin_row, None);
    // The row is still open for the next output that writes.
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    stage.finish(&mut next);
    assert_eq!(next.bytes, b"");
    assert_eq!(next.pin_row, Some(true));
}

/// A stage whose capture settles, pinning your prompt.
fn stage_settling_pinned() -> Stage {
    let mut stage = stage(SETTLES, true);
    stage.set_show(PromptShow::Pinned);
    stage
}

#[test]
fn a_prompt_pinned_with_drawing_off_keeps_what_prompts_triggers_did() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_shown(&mut out, block, None, b"", Some(b"[1020/1020HITPOINTS]"));
    assert_eq!(out.pin.as_deref(), Some(&b"[1020/1020HITPOINTS]"[..]));
    stage.finish(&mut out);
    // A repaint of the band keeps the trigger's text.
    let mut again = Output::new(false);
    stage.repaint(&mut again, None);
    assert_eq!(again.pin, None);
    // Back in the text it shows as the trigger left it.
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint(&mut back, None);
    assert_eq!(back.bytes, b"[1020/1020HITPOINTS]\r\n");
    // A trigger that hid it leaves nothing to bring back.
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_shown(&mut out, block, None, b"", None);
    stage.finish(&mut out);
    let mut again = Output::new(false);
    stage.repaint(&mut again, None);
    assert_eq!(again.pin, None, "the band stays empty");
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint(&mut back, None);
    let leftover = &back.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn in_the_text_nothing_is_held_or_pinned() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    let leftover = &out.hold;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(out.pin, None);
    assert!(!stage.swallows());
    stage.line(&mut out, b"", "", None, b"\r\n");
    assert!(out.bytes.ends_with(b"DRAWN\r\n"));
}

/// A stage that reads `pattern` and lifts your prompt.
fn lifted_stage(pattern: &str, settle: bool) -> Stage {
    let mut stage = stage(pattern, settle);
    stage.set_show(PromptShow::Lifted);
    stage
}

#[test]
fn a_lift_ends_after_the_last_visible_byte_and_keeps_your_echo_a_cell_away() {
    assert_eq!(lift_start(4), b"\x1b]7717;l;4\x07");
    assert_eq!(lift_end(4), b"\x1b]7717;e;4\x07");
    // A body that ends on a glyph gains one plain space.
    assert_eq!(
        with_lift_end(b"DRAWN", 4),
        with(&[b"DRAWN", &lift_end(4), b" "])
    );
    assert_eq!(
        with_lift_end(b"DRAWN\x1b[0m", 4),
        with(&[b"DRAWN\x1b[0m", &lift_end(4), b" "])
    );
    // One that ends on a space, or keeps a line end, gains nothing.
    assert_eq!(
        with_lift_end(b"<10hp> ", 4),
        with(&[b"<10hp> ", &lift_end(4)])
    );
    assert_eq!(
        with_lift_end(b"DRAWN\r\n", 4),
        with(&[b"DRAWN", &lift_end(4), b"\r\n"])
    );
    // The marks take no room when the text wraps.
    let marked = with(&[&lift_start(1), b"ab cd", &lift_end(1)]);
    let text = String::from_utf8(marked.clone()).unwrap();
    assert_eq!(crate::wrap::wrap_stream(&text, 5).as_bytes(), &marked[..]);
}

#[test]
fn a_lifted_prompt_carries_its_marks_around_every_line_it_shows() {
    let mut stage = lifted_stage(JAMES, false);
    let mut out = Output::new(false);
    let block = Block {
        lines: vec![
            BlockLine {
                raw: b"Tester: [===|---]".to_vec(),
                plain: "Tester: [===|---]".into(),
                end: End::Line,
            },
            BlockLine {
                raw: PROMPT.as_bytes().to_vec(),
                plain: PROMPT.into(),
                end: End::Line,
            },
        ],
        replaced: vec![1],
        values: BTreeMap::new(),
        afk: false,
        groups: Vec::new(),
    };
    stage.draw(&mut out, block.clone(), None, b"echo\r\n", "DRAWN");
    // Echoes stay outside, the tank line shown as sent inside.
    assert_eq!(
        out.bytes,
        with(&[
            b"echo\r\n",
            &lift_start(1),
            b"Tester: [===|---]\r\n",
            &mark(2),
            b"DRAWN",
            &lift_end(1),
            b" "
        ])
    );
    assert_eq!(stage.open_row().map(|r| &r.body[..]), Some(&b"DRAWN"[..]));
    // A repaint rewrites the region with the same lift's end mark.
    let mut repaint = Output::new(false);
    stage.repaint(&mut repaint, Some("NEW> "));
    assert_eq!(
        repaint.replace.map(|r| r.bytes),
        Some(with(&[&mark(3), b"NEW> ", &lift_end(1)]))
    );
    // Drawing off, the repaint lifts the line as the game sent it.
    let mut off = Output::new(false);
    stage.repaint(&mut off, None);
    assert_eq!(
        off.replace.map(|r| r.bytes),
        Some(with(&[&mark(4), PROMPT.as_bytes(), &lift_end(1), b"\r\n"]))
    );
    // Shown as sent, the whole block sits between the marks, before
    // its line end.
    let mut shown = Output::new(false);
    stage.show_as_sent(&mut shown, block, None, b"", Some(PROMPT.as_bytes()));
    assert_eq!(
        shown.bytes,
        with(&[
            &lift_start(5),
            b"Tester: [===|---]\r\n",
            PROMPT.as_bytes(),
            &lift_end(5),
            b"\r\n"
        ])
    );
}

#[test]
fn a_prompt_whole_before_its_line_end_ends_its_lift_before_it() {
    let mut stage = lifted_stage(SETTLES, true);
    let block = read(&stage, "<10hp> ", End::Line);
    let mut out = Output::new(false);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    assert_eq!(
        out.bytes,
        with(&[&lift_start(1), &mark(2), b"DRAWN", &lift_end(1), b"\r\n"])
    );
    // A prompt shown as sent that keeps the cursor after it ends on
    // its own space.
    let block = read(&stage, "<10hp> ", End::Settled);
    let mut out = Output::new(false);
    stage.show_as_sent(&mut out, block, None, b"", Some(b"<10hp> "));
    assert_eq!(out.bytes, with(&[&lift_start(3), b"<10hp> ", &lift_end(3)]));
}

#[test]
fn choosing_lifted_lifts_the_open_row_at_once() {
    let mut stage = stage(JAMES, false);
    let block = read(&stage, PROMPT, End::Line);
    let mut out = Output::new(false);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    assert!(!out.bytes.windows(8).any(|w| w == b"7717;l;1"));
    stage.set_show(PromptShow::Lifted);
    let mut lift = Output::new(false);
    stage.repaint(&mut lift, Some("DRAWN"));
    assert_eq!(
        lift.replace.map(|r| r.bytes),
        Some(with(&[
            &mark(3),
            &lift_start(2),
            b"DRAWN",
            &lift_end(2),
            b" "
        ]))
    );
    // The start sits inside the region, so a repaint writes it again.
    let mut again = Output::new(false);
    stage.repaint(&mut again, Some("NEW"));
    assert_eq!(
        again.replace.map(|r| r.bytes),
        Some(with(&[
            &mark(4),
            &lift_start(2),
            b"NEW",
            &lift_end(2),
            b" "
        ]))
    );
    // Back to the text, the row keeps its marks, and nothing moves.
    stage.set_show(PromptShow::Text);
    let mut same = Output::new(false);
    stage.repaint(&mut same, Some("NEW"));
    assert!(same.is_empty());
}

#[test]
fn an_edit_that_reads_a_line_above_the_last_takes_it_over_or_gives_it_back() {
    let hp: BTreeSet<FieldRef> = [FieldRef::new("hp")].into();
    let tank: BTreeSet<FieldRef> = [FieldRef::new("tank"), FieldRef::new("hp")].into();
    let mut stage = self::stage(JAMES, false);
    stage.set_reads(&hp);
    let mut out = Output::new(false);
    stage.draw(&mut out, tank_block(), None, b"", "DRAWN");
    stage.finish(&mut out);
    assert!(!stage.stale(View::live(Some("DRAWN"))));
    // The design starts reading the tank, so it draws the tank line
    // itself, and the one the game sent goes with the region where a
    // renderer finds it.
    stage.set_reads(&tank);
    assert!(stage.stale(View::live(Some("DRAWN"))));
    let mut over = Output::new(false);
    stage.repaint(&mut over, Some("TANK\r\nDRAWN"));
    let replace = over.replace.expect("the repaint");
    assert_eq!(replace.gen, 1);
    assert_eq!(replace.bytes, with(&[&mark(2), b"TANK\r\nDRAWN"]));
    assert_eq!(
        replace.above,
        Some(Above {
            plain: "Tester: [===|---]".into(),
            bytes: with(&[&mark(2), b"TANK\r\nDRAWN"]),
        })
    );
    // It stops reading it, so the line shows as sent again, at the
    // region's start, since nothing showed above the region.
    stage.set_reads(&hp);
    let mut back = Output::new(false);
    stage.repaint(&mut back, Some("DRAWN"));
    let replace = back.replace.expect("the repaint");
    assert_eq!(
        replace.bytes,
        with(&[b"Tester: [===|---]\r\n", &mark(3), b"DRAWN"])
    );
    assert_eq!(replace.above, None);
    // A later repaint rewrites only the region.
    let mut again = Output::new(false);
    stage.repaint(&mut again, Some("NEW"));
    let replace = again.replace.expect("the repaint");
    assert_eq!(replace.bytes, with(&[&mark(4), b"NEW"]));
    assert_eq!(replace.above, None);

    // Drawing off, the region holds the lines the design replaced as
    // sent, and the line above it moves into it the same way.
    let mut stage = self::stage(JAMES, false);
    stage.set_reads(&hp);
    let mut out = Output::new(false);
    stage.draw(&mut out, tank_block(), None, b"", "DRAWN");
    stage.set_reads(&tank);
    let mut off = Output::new(false);
    stage.repaint(&mut off, None);
    let replace = off.replace.expect("the repaint");
    let sent = with(&[
        &mark(2),
        b"Tester: [===|---]\r\n",
        PROMPT.as_bytes(),
        b"\r\n",
    ]);
    assert_eq!(
        replace.above.map(|a| (a.plain, a.bytes)),
        Some(("Tester: [===|---]".into(), sent))
    );
}

/// A block with a tank line above the prompt, which the design does
/// not read, so it shows as sent.
fn tank_block() -> Block {
    Block {
        lines: vec![
            BlockLine {
                raw: b"Tester: [===|---]".to_vec(),
                plain: "Tester: [===|---]".into(),
                end: End::Line,
            },
            BlockLine {
                raw: PROMPT.as_bytes().to_vec(),
                plain: PROMPT.into(),
                end: End::Line,
            },
        ],
        replaced: vec![1],
        values: BTreeMap::new(),
        afk: false,
        groups: vec![vec!["tank".into(), "tank_bar".into()], Vec::new()],
    }
}

#[test]
fn the_band_carries_where_each_piece_of_the_design_landed_on_it() {
    // Two pieces on the design's first row and one on its second.
    let spans = [
        span_at(0, 0, 3),
        span_at(1, 3, 2),
        Span {
            row: 1,
            ..span_at(2, 0, 3)
        },
    ];
    let rows = |out: &Output| -> Option<Vec<(usize, usize, usize)>> {
        out.pin_spans
            .as_ref()
            .map(|spans| spans.iter().map(|s| (s.piece, s.row, s.col)).collect())
    };
    // The tank line shows as sent above the design, so each piece
    // sits a row lower on the band.
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    stage.pin_view(
        &mut out,
        tank_block(),
        None,
        b"",
        drawn_view("HP 1>\r\nMN9", &spans),
    );
    assert_eq!(
        out.pin.as_deref(),
        Some(&b"Tester: [===|---]\r\nHP 1>\r\nMN9"[..])
    );
    assert_eq!(rows(&out), Some(vec![(0, 1, 0), (1, 1, 3), (2, 2, 0)]));
    stage.finish(&mut out);

    // A repaint that shows the same band with the same pieces sends
    // nothing. Pieces numbered anew go out again, bytes and all.
    let mut same = Output::new(false);
    stage.repaint_view(&mut same, drawn_view("HP 1>\r\nMN9", &spans));
    assert_eq!(same.pin, None);
    let renumbered = [
        span_at(0, 0, 5),
        Span {
            row: 1,
            ..span_at(1, 0, 3)
        },
    ];
    let mut again = Output::new(false);
    stage.repaint_view(&mut again, drawn_view("HP 1>\r\nMN9", &renumbered));
    assert!(again.pin.is_some());
    assert_eq!(rows(&again), Some(vec![(0, 1, 0), (1, 2, 0)]));

    // Drawing off and the game's own line under the card show no
    // design, so the band carries no pieces.
    let mut off = Output::new(false);
    stage.repaint_view(&mut off, View::live(None));
    assert_eq!(
        off.pin.as_deref(),
        Some(&b"Tester: [===|---]\r\n[1020/1020hp 800/800mn 930/930mv]"[..])
    );
    assert_eq!(off.pin_spans, None);
    let mut raw = Output::new(false);
    stage.repaint_view(
        &mut raw,
        View {
            live: Some("HP 1>"),
            spans: &spans,
            plain: "HP 1>",
            ..View::default()
        },
    );
    assert_eq!(raw.pin, None, "the band already shows the game's lines");
    let mut on = Output::new(false);
    stage.repaint_view(&mut on, drawn_view("HP 2>", &spans[..2]));
    assert_eq!(rows(&on), Some(vec![(0, 1, 0), (1, 1, 3)]));

    // A prompt pinned with drawing off carries none.
    let mut shown = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_shown(&mut shown, block, None, b"", Some(PROMPT.as_bytes()));
    assert!(shown.pin.is_some());
    assert_eq!(shown.pin_spans, None);

    // Choosing Pinned with the row open in the text sends the band
    // with its pieces, and leaving Pinned clears both.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    stage.draw_view(
        &mut out,
        tank_block(),
        None,
        b"",
        drawn_view("HP 1>", &spans[..2]),
    );
    stage.finish(&mut out);
    stage.set_show(PromptShow::Pinned);
    let mut pin = Output::new(false);
    stage.repaint_view(&mut pin, drawn_view("HP 1>", &spans[..2]));
    assert_eq!(rows(&pin), Some(vec![(0, 1, 0), (1, 1, 3)]));
    stage.finish(&mut pin);
    stage.set_show(PromptShow::Text);
    let mut back = Output::new(false);
    stage.repaint_view(&mut back, drawn_view("HP 1>", &spans[..2]));
    assert_eq!(back.pin.as_deref(), Some(&b""[..]));
    assert_eq!(back.pin_spans, None);
}

#[test]
fn a_change_of_place_takes_the_tank_line_above_the_region_along() {
    assert!(shows_lines(
        &["Tester: [===|".into(), "---]".into()],
        "Tester: [===|---]"
    ));
    assert!(!shows_lines(&["Tester: [===|".into()], "Tester: [===|---]"));
    assert!(!shows_lines(&[], ""));

    // In the text, then Pinned: the tank line goes with the region.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    stage.draw(&mut out, tank_block(), None, b"", "DRAWN");
    assert_eq!(
        out.bytes,
        with(&[b"Tester: [===|---]\r\n", &mark(1), b"DRAWN"])
    );
    stage.set_show(PromptShow::Pinned);
    let mut pin = Output::new(false);
    stage.repaint(&mut pin, Some("DRAWN"));
    assert_eq!(
        pin.replace,
        Some(Replace {
            gen: 1,
            bytes: Vec::new(),
            fresh: false,
            above: Some(Above {
                plain: "Tester: [===|---]".into(),
                bytes: Vec::new(),
            }),
            tail: Vec::new(),
        })
    );
    assert_eq!(pin.pin.as_deref(), Some(&b"Tester: [===|---]\r\nDRAWN"[..]));

    // In the text, then Lifted: the lift starts at the tank line when
    // a renderer finds it, and at the region otherwise.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    stage.draw(&mut out, tank_block(), None, b"", "DRAWN");
    stage.set_show(PromptShow::Lifted);
    let mut lift = Output::new(false);
    stage.repaint(&mut lift, Some("DRAWN"));
    let replace = lift.replace.expect("the repaint");
    assert_eq!(
        replace.bytes,
        with(&[&mark(3), &lift_start(2), b"DRAWN", &lift_end(2), b" "])
    );
    assert_eq!(
        replace.above,
        Some(Above {
            plain: "Tester: [===|---]".into(),
            bytes: with(&[
                &lift_start(2),
                b"Tester: [===|---]\r\n",
                &mark(3),
                b"DRAWN",
                &lift_end(2),
                b" "
            ]),
        })
    );
    // A later repaint rewrites only the region.
    let mut again = Output::new(false);
    stage.repaint(&mut again, Some("NEW"));
    assert_eq!(again.replace.and_then(|r| r.above), None);
    // With no lines above, nothing rides along.
    let mut stage = stage_settling_pinned();
    stage.set_show(PromptShow::Text);
    let block = read(&stage, "<10hp> ", End::Line);
    let mut out = Output::new(false);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.set_show(PromptShow::Pinned);
    let mut pin = Output::new(false);
    stage.repaint(&mut pin, Some("DRAWN"));
    assert_eq!(pin.replace.and_then(|r| r.above), None);
}

#[test]
fn leaving_pinned_for_lifted_brings_the_prompt_back_lifted() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    stage.set_show(PromptShow::Lifted);
    let mut back = Output::new(false);
    stage.repaint(&mut back, Some("DRAWN"));
    assert_eq!(
        back.bytes,
        with(&[&lift_start(1), &mark(2), b"DRAWN", &lift_end(1), b" "])
    );
}

/// What the open row shows with a preview on, and the live render
/// behind it.
fn preview<'a>(shown: &'a str, live: &'a str) -> View<'a> {
    View {
        shown: Some(shown),
        live: Some(live),
        ..View::default()
    }
}

#[test]
fn a_prompt_drawn_with_a_preview_carries_the_live_render_as_its_restore() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
    assert_eq!(out.bytes, with(&[&mark(1), b"LOW"]));
    assert_eq!(out.restore, Some(with(&[&mark(1), b"LIVE"])));
    let open = stage.open_row().expect("the open row");
    assert_eq!(open.body, b"LOW");
    assert_eq!(open.live.as_deref(), Some(&b"LIVE"[..]));
    // A preview that draws what the live render draws needs none.
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("SAME", "SAME"));
    assert_eq!(out.restore, None);
    assert_eq!(stage.open_row().and_then(|o| o.live.clone()), None);
}

#[test]
fn anything_written_after_a_preview_in_the_same_output_puts_the_live_render_back_first() {
    // A line after the prompt in the same read.
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
    stage.line(&mut out, b"You flee!", "You flee!", None, b"You flee!\r\n");
    assert_eq!(out.bytes, with(&[&mark(1), b"LIVE", b"You flee!\r\n"]));
    assert_eq!(out.restore, None);
    stage.finish(&mut out);
    assert_eq!(stage.open_row(), None);

    // Two prompts in one read: only the last one shows the preview.
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
    out.text(b"\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE2"));
    assert_eq!(
        out.bytes,
        with(&[&mark(2), b"LIVE", b"\r\n", &mark(3), b"LOW"])
    );
    assert_eq!(out.restore, Some(with(&[&mark(3), b"LIVE2"])));

    // A prompt that replaced its painted start, then a line.
    let mut stage = self::stage(JAMES, false);
    let mut first = Output::new(false);
    let _ = stage.paint_partial(&mut first, b"[1020/10", None);
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut second, block, Some(1), b"", preview("LOW", "LIVE"));
    assert_eq!(
        second.replace.as_ref().map(|r| r.bytes.clone()),
        Some(with(&[&mark(2), b"LOW"]))
    );
    assert_eq!(second.restore, Some(with(&[&mark(2), b"LIVE"])));
    second.text(b"\r\nThe guard arrives.\r\n");
    assert_eq!(
        second.replace.map(|r| r.bytes),
        Some(with(&[&mark(2), b"LIVE"]))
    );
    assert_eq!(second.restore, None);

    // The game's own line in the region ends its row, the live render
    // does not, so a fresh write after it starts a new row.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    let raw = View {
        shown: None,
        live: Some("LIVE"),
        ..View::default()
    };
    stage.draw_view(&mut out, block, None, b"", raw);
    assert_eq!(out.bytes, with(&[&mark(1), PROMPT.as_bytes(), b"\r\n"]));
    out.replace(9, b"later".to_vec(), true);
    assert_eq!(out.bytes, with(&[&mark(1), b"LIVE\r\nlater"]));
    assert_eq!(out.restore, None);
}

#[test]
fn a_preview_repaints_the_open_row_with_the_live_render_behind_it() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "LIVE");
    assert_eq!(out.restore, None);

    let mut low = Output::new(false);
    stage.repaint_view(&mut low, preview("LOW", "LIVE"));
    assert_eq!(
        low.replace,
        Some(Replace {
            gen: 1,
            bytes: with(&[&mark(2), b"LOW"]),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &low.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(low.restore, Some(with(&[&mark(2), b"LIVE"])));
    // The same view again writes nothing.
    let mut same = Output::new(false);
    stage.repaint_view(&mut same, preview("LOW", "LIVE"));
    assert!(same.is_empty());
    // The live render behind the preview moved, so the row carries
    // the new one.
    let mut moved = Output::new(false);
    stage.repaint_view(&mut moved, preview("LOW", "LIVE2"));
    assert_eq!(
        moved.replace.map(|r| r.bytes),
        Some(with(&[&mark(3), b"LOW"]))
    );
    assert_eq!(moved.restore, Some(with(&[&mark(3), b"LIVE2"])));
    // The card closes: the live render, with nothing to restore.
    let mut live = Output::new(false);
    stage.repaint_view(&mut live, View::live(Some("LIVE2")));
    assert_eq!(
        live.replace.map(|r| r.bytes),
        Some(with(&[&mark(4), b"LIVE2"]))
    );
    assert_eq!(live.restore, None);
    assert_eq!(stage.open_row().and_then(|o| o.live.clone()), None);
    // A preview that matches the live render after all writes it with
    // nothing to restore, so a restore a renderer still holds goes.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LOW", "LIVE"));
    let mut back = Output::new(false);
    stage.repaint_view(&mut back, View::live(Some("LOW")));
    assert_eq!(
        back.replace.map(|r| r.bytes),
        Some(with(&[&mark(2), b"LOW"]))
    );
    assert_eq!(back.restore, None);
}

#[test]
fn the_open_card_lifts_the_row_that_draws_your_design_in_the_text() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.set_card(true);
    // The card reads your codes first: the game's line shows with no
    // band.
    let mut raw = Output::new(false);
    stage.repaint_view(
        &mut raw,
        View {
            shown: None,
            live: Some("DRAWN"),
            ..View::default()
        },
    );
    assert_eq!(
        raw.replace.map(|r| r.bytes),
        Some(with(&[&mark(2), PROMPT.as_bytes(), b"\r\n"]))
    );
    // Then it draws your design, and the row lifts from its start,
    // the live render behind it too, with Lifted's space after the
    // lift, trailing space included as the addendum says, so your
    // echo stays a cell away from the band.
    let mut card = Output::new(false);
    stage.repaint_view(&mut card, preview("LABELS", "DRAWN"));
    assert_eq!(
        card.replace.map(|r| r.bytes),
        Some(with(&[
            &mark(4),
            &lift_start(3),
            b"LABELS",
            &lift_end(3),
            b" "
        ]))
    );
    assert_eq!(
        card.restore,
        Some(with(&[
            &mark(4),
            &lift_start(3),
            b"DRAWN",
            &lift_end(3),
            b" "
        ]))
    );
    // A prompt that arrives while the card is open lifts too.
    let mut next = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut next, block, None, b"", preview("LABELS", "DRAWN"));
    assert_eq!(
        next.bytes,
        with(&[&lift_start(5), &mark(6), b"LABELS", &lift_end(5), b" "])
    );
    // The card closes. The row keeps its marks, its lift starting
    // before the region as that prompt drew it, and the next prompt
    // draws in the text as before.
    stage.set_card(false);
    let mut closed = Output::new(false);
    stage.repaint_view(&mut closed, View::live(Some("DRAWN")));
    assert_eq!(
        closed.replace.map(|r| r.bytes),
        Some(with(&[&mark(7), b"DRAWN", &lift_end(5), b" "]))
    );
    let mut after = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut after, block, None, b"", "DRAWN");
    assert_eq!(after.bytes, with(&[&mark(8), b"DRAWN"]));
    // A connection that opens or closes keeps the card's state.
    stage.set_card(true);
    stage.reset();
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LABELS", "DRAWN"));
    assert!(out.bytes.starts_with(&lift_start(9)));
    // Drawing off, the game's line shows as sent with no band.
    let mut off = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.show_as_sent(&mut off, block, None, b"", Some(PROMPT.as_bytes()));
    assert!(!off.bytes.windows(7).any(|w| w == b"7717;l;"));
}

#[test]
fn a_row_the_card_lifted_keeps_lifteds_space_when_you_choose_lifted() {
    // The card borrows Lifted's band in the text, trailing space
    // included (the 2026-09-30 addendum, item 2), so a row it lifted
    // keeps your echo a cell away once you choose Lifted.
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.set_card(true);
    let mut card = Output::new(false);
    stage.repaint_view(&mut card, preview("LABELS", "DRAWN"));
    stage.set_card(false);
    let mut closed = Output::new(false);
    stage.repaint_view(&mut closed, View::live(Some("DRAWN")));
    stage.set_show(PromptShow::Lifted);
    let mut lifted = Output::new(false);
    stage.repaint(&mut lifted, Some("DRAWN"));
    let mut next = Output::new(false);
    stage.repaint(&mut next, Some("NEW"));
    let bytes = next.replace.map(|r| r.bytes).expect("a repaint");
    assert!(
        bytes.ends_with(&with(&[b"NEW", &lift_end(2), b" "])),
        "{bytes:?}"
    );
    // A prompt drawn while the card was open keeps the space in your
    // scrollback too.
    stage.set_show(PromptShow::Text);
    stage.set_card(true);
    let mut during = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut during, block, None, b"", preview("LABELS", "DRAWN"));
    let live = during.restore.expect("the live render");
    assert!(live.ends_with(b" "), "{live:?}");
    assert!(during.bytes.ends_with(b" "), "{:?}", during.bytes);
}

#[test]
fn while_the_card_reads_your_codes_the_row_shows_the_game_line_over_your_design() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    let mut raw = Output::new(false);
    stage.repaint_view(
        &mut raw,
        View {
            shown: None,
            live: Some("DRAWN"),
            ..View::default()
        },
    );
    assert_eq!(
        raw.replace.map(|r| r.bytes),
        Some(with(&[&mark(2), PROMPT.as_bytes(), b"\r\n"]))
    );
    assert_eq!(raw.restore, Some(with(&[&mark(2), b"DRAWN"])));
}

#[test]
fn a_lifted_preview_and_its_restore_both_end_with_the_lift() {
    let mut stage = lifted_stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    let mut low = Output::new(false);
    stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
    assert_eq!(
        low.replace.map(|r| r.bytes),
        Some(with(&[&mark(3), b"LOW", &lift_end(1), b" "]))
    );
    assert_eq!(
        low.restore,
        Some(with(&[&mark(3), b"DRAWN", &lift_end(1), b" "]))
    );
    // A prompt drawn while the preview lasts shows it, and puts the
    // live render back when text follows.
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw_view(&mut out, block, None, b"", preview("LOW", "DRAWN"));
    assert_eq!(
        out.bytes,
        with(&[&lift_start(4), &mark(5), b"LOW", &lift_end(4), b" "])
    );
    assert_eq!(
        out.restore,
        Some(with(&[&mark(5), b"DRAWN", &lift_end(4), b" "]))
    );
    out.text(b"\r\nmore\r\n");
    assert_eq!(
        out.bytes,
        with(&[
            &lift_start(4),
            &mark(5),
            b"DRAWN",
            &lift_end(4),
            b" ",
            b"\r\nmore\r\n"
        ])
    );
    // A lift that starts inside the region starts again in both.
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.set_show(PromptShow::Lifted);
    let mut lift = Output::new(false);
    stage.repaint(&mut lift, Some("DRAWN"));
    let mut low = Output::new(false);
    stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
    assert_eq!(
        low.replace.map(|r| r.bytes),
        Some(with(&[
            &mark(4),
            &lift_start(2),
            b"LOW",
            &lift_end(2),
            b" "
        ]))
    );
    assert_eq!(
        low.restore,
        Some(with(&[
            &mark(4),
            &lift_start(2),
            b"DRAWN",
            &lift_end(2),
            b" "
        ]))
    );
}

#[test]
fn a_pinned_preview_changes_only_the_band() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    out.text(b"room\r\n\r\n");
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_view(&mut out, block, None, b"", preview("LOW", "DRAWN"));
    assert_eq!(out.pin.as_deref(), Some(&b"LOW"[..]));
    assert_eq!(out.restore, None);
    assert_eq!(out.bytes, b"room");
    stage.finish(&mut out);
    // The card closes, and the band shows the live render.
    let mut live = Output::new(false);
    stage.repaint_view(&mut live, View::live(Some("DRAWN")));
    assert_eq!(live.pin.as_deref(), Some(&b"DRAWN"[..]));
    assert!(live.replace.is_none() && live.bytes.is_empty() && live.restore.is_none());
    // A preview on the band needs no restore either.
    let mut low = Output::new(false);
    stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
    assert_eq!(low.pin.as_deref(), Some(&b"LOW"[..]));
    assert!(low.replace.is_none() && low.bytes.is_empty() && low.restore.is_none());
    // The game's own line while the card reads your codes.
    let mut raw = Output::new(false);
    stage.repaint_view(
        &mut raw,
        View {
            shown: None,
            live: Some("DRAWN"),
            ..View::default()
        },
    );
    assert_eq!(raw.pin.as_deref(), Some(PROMPT.as_bytes()));
    assert!(raw.restore.is_none());
}

#[test]
fn a_repaint_is_due_only_when_it_would_change_what_your_prompt_shows() {
    let mut stage = stage(JAMES, false);
    assert!(!stage.stale(View::live(Some("NEW"))), "nothing drawn yet");
    assert!(!stage.repaintable());
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.finish(&mut out);
    assert!(stage.repaintable());
    assert!(!stage.stale(View::live(Some("DRAWN"))));
    assert!(stage.stale(View::live(Some("NEW"))));
    assert!(stage.stale(View::live(None)), "drawing off");
    // The live render behind a preview counts too.
    assert!(stage.stale(preview("DRAWN", "LIVE")));
    let mut low = Output::new(false);
    stage.repaint_view(&mut low, preview("LOW", "DRAWN"));
    assert!(!stage.stale(preview("LOW", "DRAWN")));
    assert!(stage.stale(preview("LOW", "DRAWN2")));
    // A closed row needs nothing.
    stage.close();
    assert!(!stage.repaintable());
    assert!(!stage.stale(View::live(Some("NEW"))));

    // The band, while pinned, whether or not text came after it.
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    assert!(stage.repaintable());
    assert!(!stage.stale(View::live(Some("DRAWN"))));
    assert!(stage.stale(View::live(Some("NEW"))));
    let mut later = Output::new(false);
    later.text(b"\r\nA guard arrives.\r\n");
    stage.finish(&mut later);
    assert!(stage.repaintable());
    assert!(stage.stale(View::live(Some("NEW"))));
    // A change of where your prompt shows waits for its own repaint.
    stage.set_show(PromptShow::Text);
    assert!(!stage.repaintable());
    assert!(!stage.stale(View::live(Some("NEW"))));
    let mut stage = self::stage(JAMES, false);
    let mut out = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.set_show(PromptShow::Lifted);
    assert!(!stage.repaintable());
    assert!(!stage.stale(View::live(Some("NEW"))));
}

#[test]
fn an_output_writes_text_when_anything_lands_in_the_text() {
    assert!(!Output::new(false).writes_text());
    assert!(!Output::new(true).writes_text());
    let mut out = Output::new(false);
    out.text(b"x");
    assert!(out.writes_text());
    // A pin alone writes nothing to the text.
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    pin_prompt(&mut stage, &mut out);
    assert!(!out.writes_text());
    let mut out = Output::new(false);
    out.replace(3, Vec::new(), false);
    assert!(out.writes_text());
}

// Collapse repeated lines. The lines are the game's own, from `fight.c`
// in the server source, with an invented name.

const DODGE: &[u8] = b"You dodge Quenby's attack.";
const PARRY: &[u8] = b"You parry Quenby's attack.";

/// Region `gen` holding `line` as a run of `count` shows it, with its
/// line end.
fn run_region(gen: u64, count: u32, line: &[u8]) -> Vec<u8> {
    with(&[&mark(gen), &shown(count, line), b"\r\n"])
}

/// `line` as a run of `count` shows it, in the default colors.
fn shown(count: u32, line: &[u8]) -> Vec<u8> {
    counted(count, line, &SgrState::default())
}

/// Offer `line` to `stage` as the session does while Collapse repeated
/// lines is on.
fn repeat(stage: &mut Stage, out: &mut Output, line: &[u8]) -> Repeat {
    let plain = String::from_utf8_lossy(line).into_owned();
    stage.repeat_line(out, line, &plain, None, line)
}

#[test]
fn the_count_draws_gray_before_the_line_from_the_second_on() {
    assert_eq!(shown(1, DODGE), DODGE);
    assert_eq!(
        shown(3, DODGE),
        b"\x1b[0m\x1b[38;5;244m(3) \x1b[39mYou dodge Quenby's attack."
    );
    // The count goes after the colors the line opens with, on its
    // background, and the line's own colors come back after it.
    let washed: &[u8] = b"\x1b[33;48;2;51;51;0mYou are hungry.\x1b[0m";
    assert_eq!(
        shown(2, washed),
        b"\x1b[0m\x1b[33;48;2;51;51;0m\x1b[38;5;244m(2) \x1b[33mYou are hungry.\x1b[0m"
    );
    let red: &[u8] = b"\x1b[1;31mYou are hungry.";
    assert_eq!(
        shown(2, red),
        b"\x1b[0m\x1b[1;31m\x1b[22;38;5;244m(2) \x1b[1;31mYou are hungry."
    );
    // A line that relies on the color an earlier line left on starts
    // in it again, and keeps it after the count.
    let mut green = SgrState::default();
    green.apply("32");
    assert_eq!(
        counted(2, b"You are hungry.", &green),
        b"\x1b[0;32m\x1b[38;5;244m(2) \x1b[32mYou are hungry."
    );
    assert!(collapsible(DODGE));
    assert!(collapsible(b"\x1b[1;31mYou are hungry.\x1b[0m"));
    assert!(!collapsible(b""));
    assert!(!collapsible(b"   \x1b[0m"));
    assert!(!collapsible(b"one\r\ntwo"));
    assert!(!collapsible(b"over\rwrite"));
}

#[test]
fn the_rules_take_fight_lines_and_leave_attack_lines_whole_at_first() {
    let hit = "Your slash hits a Blackwatch guard.";
    let battle = "A Blackwatch guard has quite a few wounds. ";
    let dodge = "You dodge Quenby's attack.";
    let rules = CollapseRules::default();
    assert_eq!(
        rules,
        CollapseRules {
            fights: true,
            attacks: false
        }
    );
    // In a fight or not, every line but an attack line collapses.
    for fighting in [false, true] {
        assert!(rules.takes(fighting, battle));
        assert!(rules.takes(fighting, dodge));
        assert!(!rules.takes(fighting, hit));
    }
    // Attack lines collapse too once you say so.
    let all = CollapseRules {
        fights: true,
        attacks: true,
    };
    assert!(all.attacks_collapse());
    for fighting in [false, true] {
        assert!(all.takes(fighting, hit));
        assert!(all.takes(fighting, battle));
    }
    // With In a fight showing every line, nothing in a fight collapses,
    // and attack lines show every line anywhere, whatever their row says.
    for attacks in [false, true] {
        let whole = CollapseRules {
            fights: false,
            attacks,
        };
        assert!(!whole.attacks_collapse());
        assert!(!whole.takes(true, battle));
        assert!(!whole.takes(true, dodge));
        assert!(!whole.takes(true, hit));
        assert!(!whole.takes(false, hit));
        assert!(whole.takes(false, dodge));
    }
}

#[test]
fn repeated_lines_in_one_output_show_once_with_the_count() {
    let mut stage = Stage::default();
    let mut out = Output::new(false);
    out.text(b"You are hungry.\r\n");
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Starts);
    assert_eq!(
        out.bytes,
        with(&[b"You are hungry.\r\n", &run_region(1, 1, DODGE)])
    );
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Joins(2));
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Joins(3));
    assert_eq!(
        out.bytes,
        with(&[b"You are hungry.\r\n", &run_region(3, 3, DODGE)])
    );
    // Another line starts a run of its own. The run before it is never
    // written again, so its mark goes, and the output keeps one.
    assert_eq!(repeat(&mut stage, &mut out, PARRY), Repeat::Starts);
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Starts);
    assert_eq!(
        out.bytes,
        with(&[
            b"You are hungry.\r\n",
            &shown(3, DODGE),
            b"\r\n",
            PARRY,
            b"\r\n",
            &run_region(5, 1, DODGE),
        ])
    );
    assert_eq!(out.replace, None);
    // A line that ends the run takes its mark out too.
    let mut stage = Stage::default();
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    stage.line(&mut out, b"", "", None, b"\r\n");
    repeat(&mut stage, &mut out, PARRY);
    assert_eq!(
        out.bytes,
        with(&[DODGE, b"\r\n\r\n", &run_region(2, 1, PARRY)])
    );
    // Text the stage did not write, such as a script's echo, leaves
    // it where it is.
    let mut stage = Stage::default();
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    out.text(b"You are hungry.\r\n");
    repeat(&mut stage, &mut out, PARRY);
    assert_eq!(
        out.bytes,
        with(&[
            &run_region(1, 1, DODGE),
            b"You are hungry.\r\n",
            &run_region(2, 1, PARRY)
        ])
    );
}

#[test]
fn a_line_that_starts_in_other_colors_starts_a_run_of_its_own() {
    // Collapse repeated lines follows the colors from line to line.
    let mut stage = Stage::default();
    stage.set_collapse(true);
    let mut out = Output::new(false);
    let green: &[u8] = b"\x1b[32mYou are hungry.";
    let hungry: &[u8] = b"You are hungry.";
    assert_eq!(repeat(&mut stage, &mut out, green), Repeat::Starts);
    // The same bytes, but the first line left green on for the next.
    assert_eq!(repeat(&mut stage, &mut out, hungry), Repeat::Starts);
    assert_eq!(repeat(&mut stage, &mut out, hungry), Repeat::Joins(2));
    let mut carry = SgrState::default();
    carry.apply("32");
    assert_eq!(stage.run_shown(), Some((counted(2, hungry, &carry), 3)));
    // A line with a color of its own that it leaves on: the next one
    // looks the same, since it opens with the same colors.
    let mut stage = Stage::default();
    stage.set_collapse(true);
    let mut out = Output::new(false);
    assert_eq!(repeat(&mut stage, &mut out, green), Repeat::Starts);
    assert_eq!(repeat(&mut stage, &mut out, green), Repeat::Joins(2));
    // One whose color comes after text that relies on the color
    // before it does not, until the color it leaves on stays.
    let mut stage = Stage::default();
    stage.set_collapse(true);
    let mut out = Output::new(false);
    let late: &[u8] = b"You are \x1b[32mhungry.";
    assert_eq!(repeat(&mut stage, &mut out, late), Repeat::Starts);
    assert_eq!(repeat(&mut stage, &mut out, late), Repeat::Starts);
    assert_eq!(repeat(&mut stage, &mut out, late), Repeat::Joins(2));
}

#[test]
fn a_run_goes_on_in_the_next_output_as_a_replace_of_its_region() {
    let mut stage = Stage::default();
    let mut first = Output::new(false);
    repeat(&mut stage, &mut first, DODGE);
    stage.finish(&mut first);
    let mut second = Output::new(false);
    assert_eq!(repeat(&mut stage, &mut second, DODGE), Repeat::Joins(2));
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 1,
            bytes: run_region(2, 2, DODGE),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    // Another one in the same output rewrites the replace.
    assert_eq!(repeat(&mut stage, &mut second, DODGE), Repeat::Joins(3));
    assert_eq!(
        second.replace.as_ref().map(|r| &r.bytes),
        Some(&run_region(3, 3, DODGE))
    );
    let leftover = &second.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    stage.finish(&mut second);
    let mut third = Output::new(false);
    assert_eq!(repeat(&mut stage, &mut third, DODGE), Repeat::Joins(4));
    assert_eq!(third.replace.as_ref().map(|r| r.gen), Some(3));
    // A line after the replace writes after the run.
    assert_eq!(repeat(&mut stage, &mut third, PARRY), Repeat::Starts);
    assert_eq!(third.bytes, run_region(5, 1, PARRY));
}

#[test]
fn anything_written_after_the_run_ends_it() {
    let started = |stage: &mut Stage| {
        let mut out = Output::new(false);
        repeat(stage, &mut out, DODGE);
        stage.finish(&mut out);
        out
    };
    // Your echo, after the output that wrote the run.
    let mut stage = Stage::default();
    let out = started(&mut stage);
    stage.local_write(out.id());
    assert_eq!(
        repeat(&mut stage, &mut Output::new(false), DODGE),
        Repeat::Starts
    );
    // An echo the session hears of late, after an output that went on
    // with the run, leaves it, since the run came after the echo.
    let mut stage = Stage::default();
    let out = started(&mut stage);
    let mut next = Output::new(false);
    assert_eq!(repeat(&mut stage, &mut next, DODGE), Repeat::Joins(2));
    stage.finish(&mut next);
    stage.local_write(out.id());
    assert_eq!(
        repeat(&mut stage, &mut Output::new(false), DODGE),
        Repeat::Joins(3)
    );
    // Output from elsewhere, such as a slash command's reply.
    let mut stage = Stage::default();
    started(&mut stage);
    assert_eq!(
        repeat(&mut stage, &mut Output::new(true), DODGE),
        Repeat::Starts
    );
    // Text in a later output, such as a script's echo.
    let mut stage = Stage::default();
    started(&mut stage);
    let mut echo = Output::new(false);
    echo.text(b"\r\nYou are hungry.\r\n");
    stage.finish(&mut echo);
    assert_eq!(
        repeat(&mut stage, &mut Output::new(false), DODGE),
        Repeat::Starts
    );
    // Text later in the same output, and before the line in a new one.
    let mut stage = Stage::default();
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    out.text(b"\r\nYou are hungry.\r\n");
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Starts);
    stage.finish(&mut out);
    let mut next = Output::new(false);
    next.text(b"\r\nYou are hungry.\r\n");
    assert_eq!(repeat(&mut stage, &mut next, DODGE), Repeat::Starts);
    // A blank line, and a line written while collapse is off.
    let mut stage = Stage::default();
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    stage.line(&mut out, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Starts);
    stage.line(
        &mut out,
        DODGE,
        "You dodge Quenby's attack.",
        None,
        b"You dodge Quenby's attack.\r\n",
    );
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Starts);
    // A new connection.
    let mut stage = Stage::default();
    started(&mut stage);
    stage.reset();
    assert_eq!(
        repeat(&mut stage, &mut Output::new(false), DODGE),
        Repeat::Starts
    );
}

#[test]
fn a_hidden_line_and_a_repaint_of_the_band_leave_the_run() {
    let mut stage = pinned_stage();
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    stage.line(&mut out, b"spam", "spam", None, b"");
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Joins(2));
    pin_prompt(&mut stage, &mut out);
    stage.finish(&mut out);
    let mut band = Output::new(false);
    stage.repaint_view(&mut band, View::live(Some("NEW")));
    assert!(!band.writes_text());
    let mut next = Output::new(false);
    stage.line(&mut next, b"spam", "spam", None, b"");
    assert_eq!(repeat(&mut stage, &mut next, DODGE), Repeat::Joins(3));
}

#[test]
fn a_prompt_left_in_the_text_ends_the_run() {
    let mut stage = stage(JAMES, false);
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    let block = read(&stage, PROMPT, End::Line);
    stage.draw(&mut out, block, None, b"", "DRAWN");
    stage.finish(&mut out);
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut next, DODGE), Repeat::Starts);
    // In the same output too, drawing off.
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, DODGE);
    let block = read(&stage, PROMPT, End::Line);
    stage.show_as_sent(&mut out, block, None, b"", Some(PROMPT.as_bytes()));
    assert_eq!(repeat(&mut stage, &mut out, DODGE), Repeat::Starts);
}

#[test]
fn a_pinned_prompt_between_repeats_leaves_the_run_and_its_held_line_ends() {
    let mut stage = pinned_stage();
    let red: &[u8] = b"\x1b[1;31mYou dodge Quenby's attack.\x1b[0m";
    let shown: &[u8] = b"\x1b[1;31mYou dodge Quenby's attack.";
    let mut out = Output::new(false);
    repeat(&mut stage, &mut out, red);
    pin_prompt(&mut stage, &mut out);
    // The next pulse in the same read: the empty line writes nothing.
    stage.line(&mut out, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut out, red), Repeat::Joins(2));
    pin_prompt(&mut stage, &mut out);
    // The run's color reset and line end wait with the prompt's.
    let plain = SgrState::default();
    assert_eq!(out.bytes, with(&[&mark(2), &counted(2, shown, &plain)]));
    assert_eq!(out.hold, b"\x1b[0m\r\n");
    stage.finish(&mut out);
    assert_eq!(out.pin_row, Some(true));

    // The next read rewrites the run without what each renderer holds
    // back, so the held line ends still follow it.
    let mut next = Output::new(false);
    stage.line(&mut next, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut next, red), Repeat::Joins(3));
    pin_prompt(&mut stage, &mut next);
    assert_eq!(
        next.replace,
        Some(Replace {
            gen: 2,
            bytes: with(&[&mark(3), &counted(3, shown, &plain)]),
            fresh: true,
            above: None,
            tail: b"\x1b[0m\r\n".to_vec(),
        })
    );
    assert!(next.bytes.is_empty() && next.hold.is_empty());
    stage.finish(&mut next);
    assert_eq!(next.pin_row, Some(true));

    // A blank line before the prompt shows, so the next pulse starts a
    // run of its own.
    let mut last = Output::new(false);
    stage.line(&mut last, b"", "", None, b"\r\n");
    repeat(&mut stage, &mut last, PARRY);
    stage.line(&mut last, b"", "", None, b"\r\n");
    pin_prompt(&mut stage, &mut last);
    stage.finish(&mut last);
    let mut after = Output::new(false);
    stage.line(&mut after, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut after, PARRY), Repeat::Starts);
}

#[test]
fn a_line_a_read_split_joins_the_run_through_the_partial_after_it() {
    let mut stage = Stage::default();
    let mut first = Output::new(false);
    repeat(&mut stage, &mut first, DODGE);
    let painted = stage.paint_partial(&mut first, b"You dodge Q", None);
    assert_eq!(painted, Some((2, 11)));
    stage.finish(&mut first);
    // The partial grows in a read of its own.
    let mut second = Output::new(false);
    let painted = stage.paint_partial(&mut second, b"You dodge Quenby", painted);
    assert_eq!(painted, Some((3, 16)));
    stage.finish(&mut second);
    let mut third = Output::new(false);
    let made = stage.repeat_line(
        &mut third,
        DODGE,
        "You dodge Quenby's attack.",
        Some(3),
        DODGE,
    );
    assert_eq!(made, Repeat::Joins(2));
    // The replace of the partial finds the run right above it and
    // writes the run there. A renderer that finds no run there writes
    // it over the partial.
    let whole = run_region(4, 2, DODGE);
    assert_eq!(
        third.replace,
        Some(Replace {
            gen: 3,
            bytes: whole.clone(),
            fresh: true,
            above: Some(Above {
                plain: "You dodge Quenby's attack.".into(),
                bytes: whole,
            }),
            tail: Vec::new(),
        })
    );
    assert_eq!(repeat(&mut stage, &mut third, DODGE), Repeat::Joins(3));
    let whole = run_region(5, 3, DODGE);
    assert_eq!(third.replace.as_ref().map(|r| &r.bytes), Some(&whole));
    assert_eq!(
        third
            .replace
            .as_ref()
            .and_then(|r| r.above.as_ref())
            .map(|a| &a.bytes),
        Some(&whole)
    );
    // A partial that becomes another line ends the run.
    stage.finish(&mut third);
    let mut fourth = Output::new(false);
    let painted = stage.paint_partial(&mut fourth, b"You parry", None);
    stage.finish(&mut fourth);
    let mut fifth = Output::new(false);
    let made = stage.repeat_line(
        &mut fifth,
        PARRY,
        "You parry Quenby's attack.",
        painted.map(|(gen, _)| gen),
        PARRY,
    );
    assert_eq!(made, Repeat::Starts);
    assert_eq!(
        fifth.replace.as_ref().map(|r| r.above.is_none()),
        Some(true)
    );
}

#[test]
fn a_pinned_prompt_a_read_split_keeps_an_empty_region_after_the_run() {
    let mut stage = pinned_stage();
    let mut first = Output::new(false);
    repeat(&mut stage, &mut first, DODGE);
    let painted = stage.paint_partial(&mut first, b"[1020/1020hp 8", None);
    stage.finish(&mut first);
    // The prompt it completes leaves the text, and its region stays
    // open with nothing in it.
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(
        &mut second,
        block,
        painted.map(|(gen, _)| gen),
        b"",
        "DRAWN",
    );
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 2,
            bytes: mark(3),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    // The next pulse in the same read writes the run where the region
    // was, with the run found right above it.
    stage.line(&mut second, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut second, DODGE), Repeat::Joins(2));
    let whole = run_region(4, 2, DODGE);
    assert_eq!(
        second.replace,
        Some(Replace {
            gen: 2,
            bytes: whole.clone(),
            fresh: true,
            above: Some(Above {
                plain: "You dodge Quenby's attack.".into(),
                bytes: whole,
            }),
            tail: Vec::new(),
        })
    );
    pin_prompt(&mut stage, &mut second);
    stage.finish(&mut second);
    assert_eq!(second.pin_row, Some(true));

    // The same in the next read.
    let mut stage = pinned_stage();
    let mut first = Output::new(false);
    repeat(&mut stage, &mut first, DODGE);
    let painted = stage.paint_partial(&mut first, b"[1020/1020hp 8", None);
    stage.finish(&mut first);
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(
        &mut second,
        block,
        painted.map(|(gen, _)| gen),
        b"",
        "DRAWN",
    );
    stage.finish(&mut second);
    let mut third = Output::new(false);
    stage.line(&mut third, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut third, DODGE), Repeat::Joins(2));
    let whole = run_region(4, 2, DODGE);
    assert_eq!(
        third.replace,
        Some(Replace {
            gen: 3,
            bytes: whole.clone(),
            fresh: true,
            above: Some(Above {
                plain: "You dodge Quenby's attack.".into(),
                bytes: whole,
            }),
            tail: Vec::new(),
        })
    );

    // A prompt that leaves the text with echoes before it ends the
    // run, and its region goes as it always went.
    let mut stage = pinned_stage();
    let mut first = Output::new(false);
    repeat(&mut stage, &mut first, DODGE);
    let painted = stage.paint_partial(&mut first, b"[1020/1020hp 8", None);
    stage.finish(&mut first);
    let mut second = Output::new(false);
    let block = read(&stage, PROMPT, End::Line);
    stage.pin_drawn(
        &mut second,
        block,
        painted.map(|(gen, _)| gen),
        b"low on mana\r\n",
        "DRAWN",
    );
    assert_eq!(second.replace.as_ref().map(|r| r.fresh), Some(true));
    stage.line(&mut second, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut second, DODGE), Repeat::Starts);
}

#[test]
fn held_lines_a_read_split_after_the_run_leave_it_whole() {
    // The PROMPT the fake Aabahran prints, whose tank line comes on a
    // line of its own before the vitals.
    let mut stage = Stage::default();
    stage.set_capture(&CaptureConfig::Aabahran(crate::config::AabahranCapture {
        prompt: crate::testkit::mud::PROMPT.to_string(),
        ..crate::config::AabahranCapture::default()
    }));
    stage.set_show(PromptShow::Pinned);
    let mut first = Output::new(false);
    repeat(&mut stage, &mut first, DODGE);
    // The tank line starts your prompt, and the read ends before the
    // rest, so it paints right after the run.
    let tank = "Tester: [===|===|===|---]";
    let offered = stage.offer(tank.as_bytes(), tank, None, End::Line);
    assert_eq!(offered.offer, Offer::Held);
    stage.end_read(&mut first);
    stage.finish(&mut first);
    // The prompt that finishes it leaves the text with it.
    let mut second = Output::new(false);
    let offered = stage.offer(PROMPT.as_bytes(), PROMPT, None, End::Line);
    let Offer::Prompt(block, painted) = offered.offer else {
        panic!("the prompt");
    };
    assert_eq!(painted, Some(2));
    stage.pin_drawn(&mut second, block, painted, b"", "DRAWN");
    assert_eq!(second.replace.as_ref().map(|r| &r.bytes), Some(&mark(3)));
    stage.line(&mut second, b"", "", None, b"\r\n");
    assert_eq!(repeat(&mut stage, &mut second, DODGE), Repeat::Joins(2));
}
