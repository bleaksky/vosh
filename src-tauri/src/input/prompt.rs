//! The `#prompt` and `#unprompt` commands, which set how Vosh reads
//! your prompt in the active profile, where it shows and whether
//! Vosh draws your design in its place.

use vosh_prompt::card::sentences::and_list;

use super::slash::parse_braced_pattern;
use super::{split_first_word, InputResult};
use crate::profile::live::Profile;

/// `#prompt {regex}`: read your prompt with a pattern. It becomes the
/// active profile's capture, `[prompt.capture] kind = "regex"`, with
/// `settle` worked out from the pattern, so an anchored pattern that ends
/// in text reads a prompt with no line end at once. Each named group
/// reads into the value of its name, so `(?<hp>\d+)` feeds Health, and a
/// pattern with none still tells Vosh where your prompt is. The switch
/// and the design stay as they are. Older builds wrote a trigger named
/// `prompt-capture` instead, which hid the prompt in every profile.
pub(super) fn slash_prompt(profile: &mut Profile, args: &str) -> InputResult {
    match split_first_word(args) {
        ("", _) => return prompt_status(profile, chrono::Local::now().fixed_offset()),
        ("game", rest) => return slash_prompt_codes(profile, rest, false),
        ("fight", rest) => return slash_prompt_codes(profile, rest, true),
        ("draw", rest) => return slash_prompt_draw(profile, rest),
        ("show", rest) => return slash_prompt_show(profile, rest),
        ("default", rest) => return slash_prompt_default(profile, rest),
        _ => {}
    }
    let Some((pattern, _rest)) = parse_braced_pattern(args) else {
        return InputResult::error("usage #prompt {regex with named groups like (?<hp>\\d+)}");
    };
    let regex = match regex::Regex::new(&pattern) {
        Ok(r) => r,
        Err(e) => return InputResult::error(format!("Vosh cannot read that pattern. {e}")),
    };
    let names: Vec<String> = regex
        .capture_names()
        .flatten()
        .map(str::to_string)
        .collect();
    let capture = vosh_prompt::config::RegexCapture {
        settle: vosh_prompt::capture::settle(&pattern),
        lines: vec![pattern],
        names: std::collections::BTreeMap::new(),
        seen_at: Some(
            chrono::Local::now()
                .fixed_offset()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        ),
        source: Some(vosh_prompt::config::CaptureSource::Typed),
    };
    let mut config = profile.prompt.config().clone();
    config.capture = vosh_prompt::CaptureConfig::Regex(capture);
    profile.set_prompt_config(config);
    InputResult::echo_line(if names.is_empty() {
        "Vosh reads your prompt with this pattern.".to_string()
    } else {
        format!(
            "Vosh reads {} from your prompt with this pattern.",
            and_list(&names)
        )
    })
}

/// `#prompt game {setting}` and `#prompt fight {setting}`: read your prompt
/// from the codes of your PROMPT or fight prompt setting, typed as you
/// type it in the game. Vosh stores it as the game would and compiles it,
/// then says what it reads and any warning. The capture becomes the
/// active profile's `kind = "aabahran"` with source typed. On the new
/// build the next Char.Prompt replaces it while the capture follows the
/// game.
fn slash_prompt_codes(profile: &mut Profile, args: &str, fight: bool) -> InputResult {
    use vosh_prompt::aabahran::{self, lex, Origin, Which};
    use vosh_prompt::card::sentences;
    use vosh_prompt::config::{AabahranCapture, CaptureSource};
    use vosh_prompt::CaptureConfig;

    let usage = if fight {
        "usage #prompt fight {your fight prompt setting}"
    } else {
        "usage #prompt game {your PROMPT setting}"
    };
    let Some((typed, _rest)) = parse_braced_pattern(args) else {
        return InputResult::error(usage);
    };
    let held = match &profile.prompt.config().capture {
        CaptureConfig::Aabahran(codes) => Some(codes.clone()),
        _ => None,
    };
    if fight && held.is_none() {
        return InputResult::echo_line(PROMPT_NONE);
    }
    if !fight && typed.trim().eq_ignore_ascii_case("off") {
        return InputResult::error(
            "That turns prompts off in the game. Type the prompt setting you use.",
        );
    }
    let which = if fight { Which::Fight } else { Which::Prompt };
    let normalized = lex::normalize(&typed, which, profile.prompt.who());
    let codes = held.unwrap_or_default();
    let (prompt, fprompt) = if fight {
        (codes.prompt.clone(), normalized.text)
    } else {
        (normalized.text, codes.fprompt.clone())
    };
    let compiled = match aabahran::compile(&prompt, &fprompt, Origin::Stored, profile.prompt.who())
    {
        Ok(compiled) => compiled,
        Err(e) => return InputResult::error(e.text),
    };
    let mut config = profile.prompt.config().clone();
    config.capture = CaptureConfig::Aabahran(AabahranCapture {
        prompt: compiled.prompt.clone(),
        fprompt: compiled.fprompt.clone(),
        follow_game: codes.follow_game,
        seen_at: Some(
            chrono::Local::now()
                .fixed_offset()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        ),
        source: Some(CaptureSource::Typed),
    });
    profile.set_prompt_config(config);
    let mut echo = vec![sentences::reads_sentence(&compiled.reads(which), fight)];
    echo.extend(
        normalized
            .warnings
            .iter()
            .chain(compiled.warnings.iter().filter(|w| w.which == which))
            .map(|w| w.text.clone()),
    );
    InputResult::echo_lines(echo)
}

/// `#prompt draw on|off`: draw your design in place of your prompt, or
/// show the game's own prompt. Turning drawing on with no design follows
/// the game, as Settings does, so Vosh draws your prompt as your codes
/// say until you change the design. The design, the place and the
/// capture stay. With no capture the echo says how to start, since Vosh
/// draws only a prompt it reads.
fn slash_prompt_draw(profile: &mut Profile, args: &str) -> InputResult {
    let draw = match args.trim().to_ascii_lowercase().as_str() {
        "on" => true,
        "off" => false,
        _ => return InputResult::error("usage #prompt draw on | off"),
    };
    let mut config = profile.prompt.config().clone();
    if config.draw != draw {
        config.draw = draw;
        if draw && config.template.is_empty() {
            config.mirror = true;
        }
        profile.set_prompt_config(config);
    }
    let mut echo = vec![if draw {
        "Drawing is on. Vosh draws your design in place of your prompt."
    } else {
        "Drawing is off. You see the game's own prompt again."
    }
    .to_string()];
    if draw && profile.prompt.config().capture.is_none() {
        echo.push(PROMPT_NONE.to_string());
    }
    InputResult::echo_lines(echo)
}

/// `#prompt show text|lifted|pinned`: where your prompt shows. In the
/// text as the game sends it, lifted on a band in the text, or pinned on
/// a band above the command line with earlier prompts out of the text.
/// It needs a capture, since Vosh finds your prompt only through one.
fn slash_prompt_show(profile: &mut Profile, args: &str) -> InputResult {
    use vosh_prompt::PromptShow;
    let Some(show) = PromptShow::parse(args) else {
        return InputResult::error("usage #prompt show text | lifted | pinned");
    };
    if profile.prompt.config().capture.is_none() {
        return InputResult::echo_line(PROMPT_NONE);
    }
    let mut config = profile.prompt.config().clone();
    config.show = show;
    profile.set_prompt_config(config);
    InputResult::echo_line(show_sentence(show))
}

/// `#prompt default`: put Vosh's default design in place of the one in
/// this profile, as your choice, so it stops following the game. The one
/// you had goes first among the earlier designs, so the card can offer it
/// back, unless it followed the game. The switch, the place and the
/// capture stay. The echo says what else it takes to see the design,
/// also when the design is the default already.
fn slash_prompt_default(profile: &mut Profile, args: &str) -> InputResult {
    if !args.trim().is_empty() {
        return InputResult::error("usage #prompt default");
    }
    let mut config = profile.prompt.config().clone();
    let had = !config.template.is_empty() && !config.mirror;
    let changed = config.use_default_design();
    let mut echo = vec![match (changed, had) {
        (false, _) => "Your design is already Vosh's default.",
        (true, true) => {
            "Your design is now Vosh's default. Vosh keeps the one you had as an earlier design."
        }
        (true, false) => "Your design is now Vosh's default.",
    }
    .to_string()];
    if config.capture.is_none() {
        echo.push(PROMPT_NONE.to_string());
    }
    if !config.draw {
        echo.push(
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
                .to_string(),
        );
    }
    if changed {
        profile.set_prompt_config(config);
    }
    InputResult::echo_lines(echo)
}

/// What `#prompt show` says once your prompt shows at `show`.
fn show_sentence(show: vosh_prompt::PromptShow) -> &'static str {
    use vosh_prompt::PromptShow;
    match show {
        PromptShow::Text => "Your prompt shows in the text.",
        PromptShow::Lifted => "Each prompt shows on a raised band in the text.",
        PromptShow::Pinned => "Your latest prompt shows pinned above the command line.",
    }
}

/// What `#prompt` says when nothing reads your prompt in this profile.
const PROMPT_NONE: &str = "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.";
/// What `#prompt` says while you have prompts off in the game.
const PROMPTS_OFF: &str =
    "You turned prompts off in the game. Type prompt in the game to turn them back on.";

/// `#prompt` alone says what reads your prompt in this profile, when it
/// last matched, whether Vosh draws, and whether you turned prompts off
/// in the game. `now` sets the clock the times read in.
pub(super) fn prompt_status(
    profile: &Profile,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> InputResult {
    use vosh_prompt::{CaptureConfig, Status};
    let engine = &profile.prompt;
    let clock = |at: chrono::DateTime<chrono::FixedOffset>| {
        at.with_timezone(now.offset()).format("%-I:%M").to_string()
    };
    let mut echo = Vec::new();
    let reads = match &engine.config().capture {
        CaptureConfig::None => None,
        CaptureConfig::Aabahran(codes) => Some(format!(
            "Vosh reads your prompt from the codes {}.",
            codes.prompt.trim_end()
        )),
        CaptureConfig::Regex(_) => {
            Some("Vosh reads your prompt with a pattern you pointed at.".to_string())
        }
    };
    match reads {
        None => echo.push(PROMPT_NONE.to_string()),
        Some(reads) => {
            let matched = match engine.last_match_at() {
                Some(at) => format!("It last matched at {}.", clock(at)),
                None => "No prompt has matched since you connected.".to_string(),
            };
            let drawing = if engine.config().draw {
                "Drawing is on."
            } else {
                "Drawing is off."
            };
            let shows = match engine.config().show {
                vosh_prompt::PromptShow::Text => "It shows in the text.",
                vosh_prompt::PromptShow::Lifted => "It shows lifted in the text.",
                vosh_prompt::PromptShow::Pinned => "It shows pinned above the command line.",
            };
            echo.push(format!("{reads} {matched} {drawing} {shows}"));
            // The game showed a PROMPT that the moved pattern could not
            // switch to.
            if let Some(kept) = engine.kept_pattern() {
                echo.push(kept);
            }
            if engine.status() == Status::NotMatching {
                let since = engine
                    .last_match_at()
                    .map_or_else(|| "you connected".to_string(), clock);
                echo.push(format!(
                    "No prompt has matched since {since}. If you changed it in the game, point at it again."
                ));
            }
        }
    }
    if engine.prompts_off() {
        echo.push(PROMPTS_OFF.to_string());
    }
    InputResult::echo_lines(echo)
}

/// `#unprompt`: stop reading your prompt in the active profile. The
/// game's prompt shows again, and the design stays saved.
pub(super) fn slash_unprompt(profile: &mut Profile) -> InputResult {
    if profile.prompt.config().capture.is_none() {
        return InputResult::echo_line("Vosh does not read your prompt in this profile.");
    }
    let mut config = profile.prompt.config().clone();
    config.capture = vosh_prompt::CaptureConfig::None;
    profile.set_prompt_config(config);
    InputResult::echo_line("Vosh stopped reading your prompt. Your design stays saved.")
}
