//! The lines the game answers `prompt` and `fprompt` with (section 3 of
//! the build spec).
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
pub enum ReplyKind {
    /// Your PROMPT setting follows the prefix.
    Prompt,
    /// Your fight prompt setting follows the prefix.
    Fight,
    /// You have no fight prompt.
    NoFight,
    /// You turned prompts off.
    Off,
}

/// A line the game answers `prompt` or `fprompt` with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    pub kind: ReplyKind,
    /// The text before the setting, for the replies that carry one.
    pub prefix: &'static str,
}

/// The replies that carry a setting after their prefix.
const SETTINGS: [(&str, ReplyKind); 5] = [
    ("Current prompt: ", ReplyKind::Prompt),
    ("Prompt set to ", ReplyKind::Prompt),
    ("Your current prompt is: ", ReplyKind::Prompt),
    ("Current fight prompt: ", ReplyKind::Fight),
    ("Fight prompt set to ", ReplyKind::Fight),
];

/// `No fight prompt set. Use 'fprompt <string>' to set one.`
const NO_FIGHT: &str = "No fight prompt set. ";
const FIGHT_CLEARED: &str = "Fight prompt cleared.";
/// What `prompt off` prints first.
pub const PROMPTS_OFF: &str = "You will no longer see prompts.";

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
pub fn reply(plain: &str) -> Option<Reply> {
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
pub fn setting(reply: Reply, raw: &[u8]) -> String {
    if reply.prefix.is_empty() {
        return String::new();
    }
    let rebuilt = colors::rebuild(&String::from_utf8_lossy(raw));
    match rebuilt.find(reply.prefix) {
        Some(at) => rebuilt[at + reply.prefix.len()..].to_string(),
        None => String::new(),
    }
}

/// True when a line you sent is `prompt off`, abbreviated or not, in any
/// case.
pub fn turns_prompts_off(sent: &str) -> bool {
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
        assert_eq!(kind("Your current prompt is: %h "), Some(ReplyKind::Prompt));
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
