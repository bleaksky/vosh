//! A PROMPT setting as the game stores it.
//!
//! [`normalize`] does what `do_prompt` and `do_fprompt`
//! (`act_info.c:2083-2146`) do to a setting you type. A setting the game
//! sends in Char.Prompt, or shows after `Current prompt:`, is already
//! stored that way, so Vosh reads it as it came and skips this step.

use super::{Warning, WarningKind, Which};

/// What `prompt all` sets (`act_info.c:2098`).
pub const PROMPT_ALL: &str = "%n%P%C<%hhp %mm %vmv> ";

/// The most characters the game keeps of a setting, `MIL - 1`.
pub const KEEP: usize = 255;

/// A setting you typed, as the game stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub text: String,
    pub warnings: Vec<Warning>,
}

/// Store a setting you typed as the game does. Trailing spaces go, then
/// the game keeps the first 255 characters, turns each `~` into `-`
/// (`smash_tilde`), and adds one space unless the setting ends in `%c`
/// in any case (`str_suffix`). `prompt all` is the stock prompt and
/// `fprompt off` clears the fight prompt. A setting that is empty or
/// only spaces stays empty, which the game draws as its fallback prompt.
///
/// `prompt off` turns prompts off and sets nothing, so the caller
/// handles it before it gets here.
pub fn normalize(typed: &str, which: Which) -> Normalized {
    let empty = || Normalized {
        text: String::new(),
        warnings: Vec::new(),
    };
    match which {
        Which::Prompt if typed == "all" => {
            return Normalized {
                text: PROMPT_ALL.to_string(),
                warnings: Vec::new(),
            };
        }
        Which::Fight if typed.eq_ignore_ascii_case("off") => return empty(),
        _ => {}
    }
    let trimmed = typed.trim_end_matches(' ');
    if trimmed.is_empty() {
        return empty();
    }
    let mut warnings = Vec::new();
    let kept = match trimmed.char_indices().nth(KEEP) {
        Some((cut, _)) => {
            warnings.push(Warning::new(
                WarningKind::Cut,
                which,
                cut..cut,
                "The game keeps the first 255 characters of your prompt. Vosh reads the same 255."
                    .to_string(),
            ));
            &trimmed[..cut]
        }
        None => trimmed,
    };
    let mut text = kept.replace('~', "-");
    if !ends_in_break(&text) {
        text.push(' ');
    }
    Normalized { text, warnings }
}

/// True when a setting ends in `%c` or `%C`, as `str_suffix("%c", …)`
/// tests it.
fn ends_in_break(text: &str) -> bool {
    text.len() >= 2 && text.as_bytes()[text.len() - 2..].eq_ignore_ascii_case(b"%c")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(typed: &str) -> String {
        normalize(typed, Which::Prompt).text
    }

    #[test]
    fn the_game_adds_one_space_after_the_trailing_ones_go() {
        assert_eq!(
            stored("[%h/%Hhp %m/%Mmn %v/%Vmv]"),
            "[%h/%Hhp %m/%Mmn %v/%Vmv] "
        );
        assert_eq!(stored("<%hhp>   "), "<%hhp> ");
        assert_eq!(stored("  <%hhp>"), "  <%hhp> ");
    }

    #[test]
    fn a_setting_that_ends_in_a_break_gets_no_space() {
        assert_eq!(
            stored("%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"),
            "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"
        );
        assert_eq!(stored("[%h]%C"), "[%h]%C");
        assert_eq!(stored("[%h]%c  "), "[%h]%c");
        // `%%c` ends in `%c` as the game compares, though it prints a
        // percent sign and a c.
        assert_eq!(stored("100%%c"), "100%%c");
    }

    #[test]
    fn prompt_all_is_the_stock_prompt() {
        assert_eq!(stored("all"), PROMPT_ALL);
        // The game compares exactly, so anything else is a setting.
        assert_eq!(stored("All"), "All ");
        assert_eq!(normalize("all", Which::Fight).text, "all ");
    }

    #[test]
    fn fprompt_off_clears_the_fight_prompt() {
        assert_eq!(normalize("off", Which::Fight).text, "");
        assert_eq!(normalize("OFF", Which::Fight).text, "");
        assert_eq!(normalize("off!", Which::Fight).text, "off! ");
    }

    #[test]
    fn a_tilde_becomes_a_dash() {
        assert_eq!(stored("~%h~"), "-%h- ");
    }

    #[test]
    fn an_empty_setting_stays_empty() {
        assert_eq!(
            normalize("", Which::Prompt),
            normalize("   ", Which::Prompt)
        );
        assert_eq!(stored(""), "");
        assert_eq!(stored("   "), "");
        assert_eq!(normalize("", Which::Fight).text, "");
    }

    #[test]
    fn the_game_keeps_the_first_255_characters() {
        let typed = format!("{}%h", "x".repeat(300));
        let got = normalize(&typed, Which::Prompt);
        assert_eq!(got.text, format!("{} ", "x".repeat(255)));
        assert_eq!(
            got.warnings,
            [Warning::new(
                WarningKind::Cut,
                Which::Prompt,
                255..255,
                "The game keeps the first 255 characters of your prompt. Vosh reads the same 255."
                    .into(),
            )]
        );
        // Exactly 255 is kept whole.
        let typed = "y".repeat(255);
        let got = normalize(&typed, Which::Fight);
        assert_eq!(got.text, format!("{typed} "));
        assert!(got.warnings.is_empty());
        // The count is in characters, and a cut that ends in spaces keeps
        // them as the game does.
        let typed = format!("{}{}zz", "é".repeat(250), " ".repeat(10));
        let got = normalize(&typed, Which::Prompt);
        assert_eq!(got.text, format!("{}{}", "é".repeat(250), " ".repeat(6)));
        assert_eq!(got.warnings[0].span, 505..505);
    }

    /// The Char.Prompt fixtures, which carry settings as the game stores
    /// them.
    const CHAR_PROMPT: [&str; 3] = [
        include_str!("../../../../fixtures/gmcp/aabahran/char-prompt.gmcp"),
        include_str!("../../../../fixtures/gmcp/aabahran/char-prompt-off.gmcp"),
        include_str!("../../../../fixtures/gmcp/aabahran/char-prompt-fight.gmcp"),
    ];

    #[test]
    fn a_setting_the_game_stored_is_already_normal() {
        for file in CHAR_PROMPT {
            let (package, json) = file.trim_end().split_once(' ').expect("a package and JSON");
            assert_eq!(package, "Char.Prompt");
            let data: serde_json::Value = serde_json::from_str(json).expect("JSON");
            for (key, which) in [("prompt", Which::Prompt), ("fprompt", Which::Fight)] {
                let setting = data[key].as_str().expect("a string");
                let got = normalize(setting, which);
                assert_eq!(got.text, setting, "{key} in {file}");
                assert!(got.warnings.is_empty(), "{key} in {file}");
            }
        }
    }
}
