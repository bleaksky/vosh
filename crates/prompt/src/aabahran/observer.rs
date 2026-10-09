//! The lines the game answers `prompt` and `fprompt` with.
//!
//! Without Char.Prompt this session, Vosh learns your settings from what
//! the game prints within 2 s after one of your own sends, and from the
//! same lines in your log. Each reply is matched anchored on the plain
//! line. The setting after it shows with your backtick colors turned into
//! color, so it is rebuilt from the raw bytes.
//!
//! `prompt off` on the older builds prints "You will no longer see
//! prompts." and then `Prompt set to` with a buffer it never filled, so
//! neither your own `prompt off` nor that line may set anything.

use super::colors;

/// What a reply says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyKind {
    /// Your PROMPT setting follows the prefix. `prompt` printed it, and
    /// every `prompt` but `prompt off` turns prompts on.
    Prompt,
    /// Your PROMPT setting follows the prefix. `channels` printed it
    /// (`act_comm.c:335`), which turns nothing on. After `prompt off` on
    /// an older build it shows the buffer that reply left unfilled.
    Channels,
    /// Your fight prompt setting follows the prefix.
    Fight,
    /// You have no fight prompt.
    NoFight,
    /// You turned prompts off.
    Off,
}

/// A line the game answers `prompt` or `fprompt` with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reply {
    pub kind: ReplyKind,
    /// The text before the setting, for the replies that carry one.
    pub prefix: &'static str,
}

/// The replies that carry a setting after their prefix.
const SETTINGS: [(&str, ReplyKind); 5] = [
    ("Current prompt: ", ReplyKind::Prompt),
    ("Prompt set to ", ReplyKind::Prompt),
    ("Your current prompt is: ", ReplyKind::Channels),
    ("Current fight prompt: ", ReplyKind::Fight),
    ("Fight prompt set to ", ReplyKind::Fight),
];

/// `No fight prompt set. Use 'fprompt <string>' to set one.`
const NO_FIGHT: &str = "No fight prompt set. ";
const FIGHT_CLEARED: &str = "Fight prompt cleared.";
/// What `prompt off` prints first.
pub(crate) const PROMPTS_OFF: &str = "You will no longer see prompts.";

/// Every prefix a reply starts with, for a log query that looks only at
/// the lines that might be one.
pub const PREFIXES: [&str; 8] = [
    "Current prompt: ",
    "Prompt set to ",
    "Your current prompt is: ",
    "Current fight prompt: ",
    "Fight prompt set to ",
    NO_FIGHT,
    FIGHT_CLEARED,
    PROMPTS_OFF,
];

/// The reply a plain line is, if any.
pub(crate) fn reply(plain: &str) -> Option<Reply> {
    for (prefix, kind) in SETTINGS {
        if plain.starts_with(prefix) {
            return Some(Reply { kind, prefix });
        }
    }
    if plain.starts_with(NO_FIGHT) || plain.trim_end() == FIGHT_CLEARED {
        return Some(Reply {
            kind: ReplyKind::NoFight,
            prefix: "",
        });
    }
    (plain.trim_end() == PROMPTS_OFF).then_some(Reply {
        kind: ReplyKind::Off,
        prefix: "",
    })
}

/// The setting a reply shows, as the game stores it: the text after the
/// prefix, with the colors the game made of your backtick codes turned
/// back into them. Empty for a reply that carries none.
pub(crate) fn setting(reply: Reply, raw: &[u8]) -> String {
    if reply.prefix.is_empty() {
        return String::new();
    }
    let rebuilt = colors::rebuild(&String::from_utf8_lossy(raw));
    match rebuilt.find(reply.prefix) {
        Some(at) => rebuilt[at + reply.prefix.len()..].to_string(),
        None => String::new(),
    }
}

/// How long after your send its reply counts, in milliseconds. In the
/// log it is also how close the reply to `prompt off` follows the line
/// it prints first.
pub(crate) const WINDOW_MS: i64 = 2_000;

/// A reply line from your log.
#[derive(Debug, Clone, Copy)]
pub struct Logged<'a> {
    pub text: &'a str,
    /// The line as the game sent it, when the log kept it.
    pub raw: Option<&'a [u8]>,
    /// Milliseconds since the epoch.
    pub ts_ms: i64,
}

/// The settings a session of your log shows last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub prompt: String,
    /// The newest fight prompt the session shows, if any.
    pub fprompt: Option<String>,
    /// When the game showed the prompt.
    pub at_ms: i64,
}

/// True when the reply at `i`, a `Prompt set to`, answers `prompt off`:
/// the older builds print it right after "You will no longer see
/// prompts."
fn answers_off(lines: &[Logged<'_>], i: usize) -> bool {
    lines.get(i + 1).is_some_and(|older| {
        reply(older.text).is_some_and(|r| r.kind == ReplyKind::Off)
            && lines[i].ts_ms - older.ts_ms <= WINDOW_MS
    })
}

/// True when prompts were off as the game printed line `i`: the newest
/// `prompt` reply before it turned them off.
fn off_at(lines: &[Logged<'_>], i: usize) -> bool {
    for (j, older) in lines.iter().enumerate().skip(i + 1) {
        match reply(older.text).map(|r| r.kind) {
            Some(ReplyKind::Off) => return true,
            Some(ReplyKind::Prompt) => return answers_off(lines, j),
            _ => {}
        }
    }
    false
}

/// The newest PROMPT setting among one session's reply lines, newest
/// first, with the newest fight prompt of the session. A `Prompt set
/// to` right after "You will no longer see prompts." is the reply to
/// `prompt off` on the older builds and sets nothing, and so is what
/// `channels` shows while prompts are off.
pub fn latest(lines: &[Logged<'_>]) -> Option<Found> {
    let fight = |line: &Logged<'_>| {
        let found = reply(line.text)?;
        matches!(found.kind, ReplyKind::Fight | ReplyKind::NoFight)
            .then(|| setting(found, line.raw.unwrap_or(line.text.as_bytes())))
    };
    let mut fprompt = None;
    for (i, line) in lines.iter().enumerate() {
        let Some(found) = reply(line.text) else {
            continue;
        };
        match found.kind {
            ReplyKind::Fight | ReplyKind::NoFight => {
                if fprompt.is_none() {
                    fprompt = fight(line);
                }
            }
            ReplyKind::Off => {}
            ReplyKind::Prompt | ReplyKind::Channels => {
                let off = if found.kind == ReplyKind::Channels {
                    off_at(lines, i)
                } else {
                    answers_off(lines, i)
                };
                if off {
                    continue;
                }
                let fprompt = fprompt.or_else(|| lines[i + 1..].iter().find_map(fight));
                return Some(Found {
                    prompt: setting(found, line.raw.unwrap_or(line.text.as_bytes())),
                    fprompt,
                    at_ms: line.ts_ms,
                });
            }
        }
    }
    None
}

/// True when a line you sent is `prompt off`, abbreviated or not, in any
/// case.
pub(crate) fn turns_prompts_off(sent: &str) -> bool {
    let mut words = sent.split_whitespace();
    let (Some(command), Some(argument), None) = (words.next(), words.next(), words.next()) else {
        return false;
    };
    "prompt".starts_with(&command.to_ascii_lowercase()) && argument.eq_ignore_ascii_case("off")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_reply_is_read_anchored() {
        let kind = |line: &str| reply(line).map(|r| r.kind);
        assert_eq!(kind("Current prompt: %h "), Some(ReplyKind::Prompt));
        assert_eq!(kind("Prompt set to %h "), Some(ReplyKind::Prompt));
        assert_eq!(
            kind("Your current prompt is: %h "),
            Some(ReplyKind::Channels)
        );
        assert_eq!(kind("Current fight prompt: %h "), Some(ReplyKind::Fight));
        assert_eq!(kind("Fight prompt set to %h "), Some(ReplyKind::Fight));
        assert_eq!(
            kind("No fight prompt set. Use 'fprompt <string>' to set one."),
            Some(ReplyKind::NoFight)
        );
        assert_eq!(kind("Fight prompt cleared."), Some(ReplyKind::NoFight));
        assert_eq!(
            kind("You will no longer see prompts."),
            Some(ReplyKind::Off)
        );
        // Quoted, or with more after it, it is not the game's reply.
        assert_eq!(kind("Tester says 'Prompt set to %h'"), None);
        assert_eq!(kind("Fight prompt cleared. Or not."), None);
        assert_eq!(kind("You will no longer see prompts. Really."), None);
        assert_eq!(kind(""), None);
    }

    #[test]
    fn the_setting_comes_back_as_you_typed_it() {
        let found = reply("Prompt set to %h ").unwrap();
        assert_eq!(setting(found, b"Prompt set to %h "), "%h ");
        // The game turned `1 and `` into color.
        let raw = b"Fight prompt set to \x1b[0;31m%h\x1b[0;0mhp [%p] > ";
        let found = reply(&vosh_plain(raw)).unwrap();
        assert_eq!(setting(found, raw), "`1%h``hp [%p] > ");
        // A tilde shows for `- and a backtick for `=.
        let found = reply("Current prompt: a~b`c ").unwrap();
        assert_eq!(setting(found, b"Current prompt: a~b`c "), "a`-b`=c ");
        // A color before the prefix stays out of the setting.
        let raw = b"\x1b[0;37mCurrent prompt: %h ";
        let found = reply(&vosh_plain(raw)).unwrap();
        assert_eq!(setting(found, raw), "%h ");
        let found = reply("Fight prompt cleared.").unwrap();
        assert_eq!(setting(found, b"Fight prompt cleared."), "");
    }

    /// The plain text of a raw line, SGR removed.
    fn vosh_plain(raw: &[u8]) -> String {
        let text = String::from_utf8_lossy(raw);
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for p in chars.by_ref() {
                    if p == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn logged(text: &str, ts_ms: i64) -> Logged<'_> {
        Logged {
            text,
            raw: None,
            ts_ms,
        }
    }

    #[test]
    fn the_newest_setting_in_a_session_wins() {
        let lines = [
            logged("Current prompt: %h %m ", 500),
            logged("Fight prompt set to %h> ", 400),
            logged("Prompt set to %h ", 300),
        ];
        assert_eq!(
            latest(&lines),
            Some(Found {
                prompt: "%h %m ".into(),
                fprompt: Some("%h> ".into()),
                at_ms: 500,
            })
        );
        // The fight prompt may be newer than the prompt.
        let lines = [
            logged("Fight prompt cleared.", 600),
            logged("Current prompt: %h ", 500),
        ];
        assert_eq!(latest(&lines).and_then(|f| f.fprompt), Some(String::new()));
        assert_eq!(latest(&[logged("Fight prompt set to %h ", 1)]), None);
        assert_eq!(latest(&[]), None);
    }

    #[test]
    fn the_reply_to_prompt_off_in_the_log_sets_nothing() {
        let lines = [
            logged("Prompt set to \u{1}garbage", 1_010),
            logged("You will no longer see prompts.", 1_000),
            logged("Prompt set to %h ", 500),
        ];
        assert_eq!(latest(&lines).map(|f| f.prompt), Some("%h ".into()));
        // Long after it, a Prompt set to is your own.
        let lines = [
            logged("Prompt set to %m ", 9_000),
            logged("You will no longer see prompts.", 1_000),
        ];
        assert_eq!(latest(&lines).map(|f| f.prompt), Some("%m ".into()));
    }

    #[test]
    fn channels_after_prompt_off_in_the_log_sets_nothing() {
        // channels shows the buffer an older build left unfilled.
        let lines = [
            logged("Your current prompt is: \u{1}\u{2}", 5_000),
            logged("Prompt set to \u{1}\u{2}", 1_010),
            logged("You will no longer see prompts.", 1_000),
            logged("Prompt set to %h ", 500),
        ];
        assert_eq!(latest(&lines).map(|f| f.prompt), Some("%h ".into()));
        // The same after the new build, which prints only the first line.
        let lines = [
            logged("Your current prompt is: %m ", 5_000),
            logged("You will no longer see prompts.", 1_000),
            logged("Prompt set to %h ", 500),
        ];
        assert_eq!(latest(&lines).map(|f| f.prompt), Some("%h ".into()));
        // With prompts on it shows your setting.
        let lines = [
            logged("Your current prompt is: %m ", 5_000),
            logged("Prompt set to %h ", 500),
        ];
        assert_eq!(latest(&lines).map(|f| f.prompt), Some("%m ".into()));
        assert_eq!(
            latest(&[logged("Your current prompt is: %m ", 1)]).map(|f| f.prompt),
            Some("%m ".into())
        );
    }

    #[test]
    fn prompt_off_is_found_in_any_spelling() {
        for sent in [
            "prompt off",
            "PROMPT OFF",
            "prom off",
            "p Off",
            "  prompt   off  ",
        ] {
            assert!(turns_prompts_off(sent), "{sent}");
        }
        for sent in [
            "prompt",
            "prompt %h",
            "prompts off",
            "prompt off now",
            "fprompt off",
            "chan off",
            "",
        ] {
            assert!(!turns_prompts_off(sent), "{sent}");
        }
    }
}
