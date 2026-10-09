//! The kind of a row an older build wrote, which stored none, read from its
//! text. Your prompt is a line the prompt capture of the profile reads, a
//! line you sent starts with `> ` and kept no bytes, and a channel's line has
//! the shape of the game's format strings in `act_comm.c` and `languages.c`.
//! Text alone misses a prompt you changed since, and reads a faction's line
//! as a clan's, since the two print alike.
//!
//! The rows before the log's first prompt are outside play, since the
//! game prints no prompt until you play, and so are the rows after you
//! step away to the account menu, until the next prompt.

use std::sync::OnceLock;

use regex::Regex;
use vosh_log::{LineKind, SceneLine};
use vosh_prompt::capture::Recognizer;

use crate::session::log_kinds::LogKinds;

/// The line the game prints as you step away to the account menu.
const LEFT_PLAY: &str = "You step away from the Forsaken Lands and return to your account menu.";

/// Each channel and the shapes of its lines, with the speaker before the
/// quote. A tell you send and the reply to `replay` come from
/// [`LogKinds`].
const SHAPES: &[(&str, &str)] = &[
    (
        "tell",
        r"^[^']+? tells you(?: in [^']+?| in a foreign tongue)? '",
    ),
    (
        "gtell",
        r"^(?:You tell your group|[^']+? tells the group)(?: in [^']+?)? '",
    ),
    (
        "yell",
        r"^(?:You (?:try to )?yell|[^']+? (?:yells|tries to yell)) '",
    ),
    (
        "say",
        r"^(?:You say|(?:\[[^\]]+\] )?[^']+? (?:says|tries to say))(?: in [^']+?)? '",
    ),
    ("pray", r"^(?:You pray|[^']+? prays) '"),
    ("newbie", r"^[^']+? NEWBIE chats: '"),
    ("immortal", r"^[^']+? IMM_TALKS: "),
    ("imp", r"^[^']+? IMP_TALKS: "),
    ("cabal", r"^\[[^\]]+\] [^']*?: '"),
    ("clan", r"^\[[^\]]+\][^ ][^']*?: '"),
];

fn shapes() -> &'static [(&'static str, Regex)] {
    static SHAPES_RE: OnceLock<Vec<(&str, Regex)>> = OnceLock::new();
    SHAPES_RE.get_or_init(|| {
        SHAPES
            .iter()
            .map(|(name, shape)| (*name, Regex::new(shape).expect("a channel shape compiles")))
            .collect()
    })
}

/// The channel whose line `plain` is shaped like, if any.
fn channel_of(plain: &str) -> Option<&'static str> {
    shapes()
        .iter()
        .find(|(_, shape)| shape.is_match(plain))
        .map(|(name, _)| *name)
}

/// What the rows of a span are, older rows read from their text. `starts_log`
/// says the span starts at the log's first row, so the rows before its
/// first prompt are outside play. `prompt` reads your prompt, when the
/// profile has a capture.
pub(super) fn kinds(
    lines: &[SceneLine],
    starts_log: bool,
    prompt: Option<&Recognizer>,
) -> Vec<LineKind> {
    let mut tags = LogKinds::default();
    let mut playing = !(starts_log && prompt.is_some());
    lines
        .iter()
        .map(|line| {
            if let Some(kind) = &line.kind {
                return kind.clone();
            }
            if line.raw.is_none() && line.text.starts_with("> ") {
                return tags.sent(&line.text.as_bytes()[2..], playing);
            }
            if prompt.is_some_and(|p| p.line(&line.text).is_some()) {
                playing = true;
                tags.prompt();
                return LineKind::Prompt;
            }
            if line.text.trim() == LEFT_PLAY {
                playing = false;
            }
            match tags.line(&line.text, playing) {
                LineKind::Text => channel_of(&line.text)
                    .map_or(LineKind::Text, |name| LineKind::Channel(name.to_string())),
                kind => kind,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_channel_reads_from_the_game_format() {
        let cases = [
            (
                "Tolliver tells you '[Exits: north east south west]'",
                Some("tell"),
            ),
            (
                "Tolliver tells you in elvish 'The day has begun.'",
                Some("tell"),
            ),
            (
                "Tolliver tells the group 'The day has begun.'",
                Some("gtell"),
            ),
            ("You tell your group 'The day has begun.'", Some("gtell")),
            ("Tolliver yells 'WiZNET 08:20:01: TICK!'", Some("yell")),
            ("You yell 'The day has begun.'", Some("yell")),
            ("Tolliver says 'The day has begun.'", Some("say")),
            ("You say 'The day has begun.'", Some("say")),
            (
                "A werebeast says in Beastial 'The day has begun.'",
                Some("say"),
            ),
            ("You pray 'The day has begun.'", Some("pray")),
            ("Tolliver prays 'The day has begun.'", Some("pray")),
            (
                "Tolliver NEWBIE chats: 'The night has begun.'",
                Some("newbie"),
            ),
            ("Tolliver IMM_TALKS: The day has begun.", Some("immortal")),
            ("Tolliver IMP_TALKS: The day has begun.", Some("imp")),
            (
                "[Knight] (Squire) Tolliver: 'The day has begun.'",
                Some("cabal"),
            ),
            ("[Blackwatch]Tolliver: 'The day has begun.'", Some("clan")),
            ("Maren walks in.", None),
            ("Maren says 'Tolliver tells you'", Some("say")),
            ("[Exits: east west]", None),
        ];
        for (line, channel) in cases {
            assert_eq!(channel_of(line), channel, "{line}");
        }
    }

    fn line(text: &str, sent: bool) -> SceneLine {
        SceneLine {
            id: 0,
            ts_ms: 0,
            text: text.to_string(),
            raw: (!sent).then(|| text.as_bytes().to_vec()),
            kind: None,
        }
    }

    #[test]
    fn older_rows_read_their_kind_from_their_text() {
        let capture = vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
            lines: vec![r"^<\d+hp \d+m \d+mv> $".into()],
            ..Default::default()
        });
        let prompt = Recognizer::compile(&capture);
        let lines = [
            line("Abandon hope, all ye who enter here...", false),
            line("> orla", true),
            line("<1020hp 800m 930mv> ", false),
            line("> look", true),
            line("Maren walks in.", false),
            line("Tolliver NEWBIE chats: 'The night has begun.'", false),
            line("> tell tolliver hello", true),
            line("You tell Tolliver '[Exits: north east south west]'", false),
            line(LEFT_PLAY, false),
            line("> 2", true),
        ];
        let tell = LineKind::Channel("tell".into());
        assert_eq!(
            kinds(&lines, true, prompt.as_ref()),
            [
                LineKind::Login,
                LineKind::Login,
                LineKind::Prompt,
                LineKind::Sent,
                LineKind::Text,
                LineKind::Channel("newbie".into()),
                LineKind::Sent,
                tell,
                LineKind::Login,
                LineKind::Login,
            ]
        );
        // A span inside the log starts in play, and a stored kind wins.
        let mut stored = line("Maren walks in.", false);
        stored.kind = Some(LineKind::Channel("say".into()));
        assert_eq!(
            kinds(
                &[line("Maren walks in.", false), stored],
                false,
                prompt.as_ref()
            ),
            [LineKind::Text, LineKind::Channel("say".into())]
        );
    }
}
