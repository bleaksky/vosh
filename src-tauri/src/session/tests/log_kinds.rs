//! What each row the session logs is, for Save a scene (Q9). The lines
//! are game text from fixtures/room-colors.

use serde_json::json;
use vosh_log::{LineKind, LogEntry};

use crate::session::log_kinds::LogKinds;

const SAY: &str = "Tolliver says 'The day has begun.'";
const TELL: &str = "Tolliver tells you '[Exits: north east south west]'";
const MAREN: &str = "Maren walks in.";

fn say() -> serde_json::Value {
    json!({"channel": "say", "speaker": "Tolliver", "text": "The day has begun."})
}

fn tell() -> serde_json::Value {
    json!({"channel": "tell", "speaker": "Tolliver", "text": "[Exits: north east south west]"})
}

fn channel(name: &str) -> LineKind {
    LineKind::Channel(name.to_string())
}

fn row(text: &str, kind: LineKind) -> LogEntry {
    LogEntry {
        session_id: 1,
        ts_ms: 0,
        text: text.to_string(),
        raw: Some(text.as_bytes().to_vec()),
        kind,
    }
}

#[test]
fn a_packet_ahead_of_its_line_waits_past_the_lines_between() {
    let mut kinds = LogKinds::default();
    kinds.packet(&say(), &mut []);
    kinds.packet(&tell(), &mut []);
    assert_eq!(kinds.line(MAREN, true), LineKind::Text);
    assert_eq!(kinds.line(TELL, true), channel("tell"));
    assert_eq!(kinds.line(SAY, true), channel("say"));
    // Each packet pairs once.
    assert_eq!(kinds.line(SAY, true), LineKind::Text);
}

#[test]
fn a_packet_after_its_line_names_the_row_since_the_last_prompt() {
    let mut kinds = LogKinds::default();
    let mut rows = vec![
        row(MAREN, kinds.line(MAREN, true)),
        row(SAY, kinds.line(SAY, true)),
    ];
    kinds.packet(&say(), &mut rows);
    assert_eq!(rows[0].kind, LineKind::Text);
    assert_eq!(rows[1].kind, channel("say"));
    // A packet whose line is not there yet waits for it.
    kinds.packet(&tell(), &mut rows);
    assert_eq!(kinds.line(TELL, true), channel("tell"));
}

#[test]
fn a_prompt_forgets_a_packet_that_found_no_line() {
    let mut kinds = LogKinds::default();
    kinds.packet(&say(), &mut []);
    kinds.prompt();
    assert_eq!(kinds.line(SAY, true), LineKind::Text);
}

#[test]
fn your_own_line_reads_you_where_the_packet_names_you() {
    let mut kinds = LogKinds::default();
    kinds.packet(
        &json!({"channel": "say", "speaker": "Orla", "text": "The day has begun."}),
        &mut [],
    );
    assert_eq!(
        kinds.line("You say 'The day has begun.'", true),
        channel("say")
    );
    // Someone the listener cannot see reads capitalized in the line.
    kinds.packet(
        &json!({"channel": "say", "speaker": "someone", "text": "The day has begun."}),
        &mut [],
    );
    assert_eq!(
        kinds.line("Someone says 'The day has begun.'", true),
        channel("say")
    );
}

#[test]
fn a_tell_you_send_is_a_tell_and_one_to_your_group_is_not() {
    let mut kinds = LogKinds::default();
    let sent = "You tell Tolliver '[Exits: north east south west]'";
    assert_eq!(kinds.line(sent, true), channel("tell"));
    let projected = "You project to Tolliver in elvish '[Exits: north east south west]'";
    assert_eq!(kinds.line(projected, true), channel("tell"));
    let group = "You tell your group '[Exits: north east south west]'";
    assert_eq!(kinds.line(group, true), LineKind::Text);
}

#[test]
fn the_reply_to_replay_is_a_tell_until_the_next_prompt() {
    let mut kinds = LogKinds::default();
    assert_eq!(kinds.sent(b"replay\r\n", true), LineKind::Sent);
    assert_eq!(kinds.line(TELL, true), channel("tell"));
    assert_eq!(kinds.line(MAREN, true), channel("tell"));
    kinds.prompt();
    assert_eq!(kinds.line(MAREN, true), LineKind::Text);
    // The game reads rep and repl as reply.
    kinds.sent(b"repl\r\n", true);
    assert_eq!(kinds.line(MAREN, true), LineKind::Text);
    kinds.sent(b"repla\r\n", true);
    assert_eq!(kinds.line(MAREN, true), channel("tell"));
}

#[test]
fn every_line_outside_play_is_login() {
    let mut kinds = LogKinds::default();
    kinds.packet(&say(), &mut []);
    assert_eq!(kinds.line(SAY, false), LineKind::Login);
    assert_eq!(kinds.sent(b"orla\r\n", false), LineKind::Login);
    assert_eq!(LogKinds::prompt_line(false), LineKind::Login);
    assert_eq!(LogKinds::prompt_line(true), LineKind::Prompt);
    // The packet waits for its line in play.
    assert_eq!(kinds.line(SAY, true), channel("say"));
}

#[test]
fn a_packet_after_its_line_names_the_newest_row_that_holds_it() {
    // An earlier line of the pulse holds the speaker and the short text
    // too, and the say is the line right before its packet.
    let yell = "Tolliver yells 'WiZNET 08:20:01: TICK!'";
    let say = "Tolliver says 'TICK!'";
    let mut kinds = LogKinds::default();
    let mut rows = vec![
        row(yell, kinds.line(yell, true)),
        row(say, kinds.line(say, true)),
    ];
    kinds.packet(
        &json!({"channel": "say", "speaker": "Tolliver", "text": "TICK!"}),
        &mut rows,
    );
    assert_eq!(rows[0].kind, LineKind::Text);
    assert_eq!(rows[1].kind, channel("say"));
}
