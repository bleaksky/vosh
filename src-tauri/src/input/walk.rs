//! `#walk`, the speedwalk. The input pipeline reads the line and hands
//! the walker in the session a [`WalkCommand`] through
//! [`InputResult::walk`]. The walker sends the steps one at a time.
//!
//! The steps are `part { [space] part }`, where a part is a direction,
//! `n`, `e`, `s`, `w`, `u` or `d` in either case, with an optional count
//! from 1 to 99 before it. Full words like `north` are refused, so a typo
//! never walks you somewhere. A walk takes at most [`MAX_STEPS`] steps.
//!
//! A typed line, an alias expansion, a macro command or a `;` piece of a
//! typed line that starts with `#walk` runs it. What follows `#walk` in
//! the same line waits for the walk to end, so it runs where the walk
//! takes you.

use vosh_automation::alias::{ExpandError, ExpandStep};

use super::{split_first_word, InputResult};
use crate::profile::live::Profile;
use crate::session::connection::Connection;

/// The most steps one walk takes.
pub(crate) const MAX_STEPS: usize = 200;

/// The most times a count repeats a direction.
const MAX_COUNT: u32 = 99;

/// One of the six ways the game moves you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Dir {
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl Dir {
    /// The direction a letter names, in either case.
    pub(crate) fn from_letter(c: char) -> Option<Self> {
        match c.to_ascii_lowercase() {
            'n' => Some(Self::North),
            'e' => Some(Self::East),
            's' => Some(Self::South),
            'w' => Some(Self::West),
            'u' => Some(Self::Up),
            'd' => Some(Self::Down),
            _ => None,
        }
    }

    /// The one letter Vosh sends for the step. `comm.c:1509` skips the
    /// spam count for one letter commands.
    pub(crate) fn letter(self) -> char {
        match self {
            Self::North => 'n',
            Self::East => 'e',
            Self::South => 's',
            Self::West => 'w',
            Self::Up => 'u',
            Self::Down => 'd',
        }
    }
}

/// The rooms a walk planned on the map passes through: the room it
/// starts in, and the room each step should reach, one for each step.
/// The walker drops a plan made from a room you have since left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) start: i64,
    pub(crate) rooms: Vec<i64>,
}

/// The steps of one walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WalkPlan {
    pub(crate) steps: Vec<Dir>,
    /// The rooms a click planned, or None for a `#walk` line, whose
    /// steps the walker checks against the tiles the game sends.
    pub(crate) route: Option<Route>,
}

/// What a `#walk` line asks of the walker in the session. `rest` is
/// what followed `#walk` in its line, aliases already expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WalkCommand {
    /// Walk `plan`. `rest` goes out once you arrive, and drops when the
    /// walk stops early.
    Start {
        plan: WalkPlan,
        rest: Vec<ExpandStep>,
    },
    /// `#walk stop`, or Esc with `key` set, which says nothing when you
    /// are not walking. `rest` runs right after.
    Stop { key: bool, rest: Vec<ExpandStep> },
    /// `#walk` alone says how many steps are left. `rest` runs right
    /// after.
    Status { rest: Vec<ExpandStep> },
}

/// Why the steps of a `#walk` line do not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StepsError {
    /// The text Vosh cannot read.
    Unreadable(String),
    /// More than [`MAX_STEPS`] steps.
    TooMany,
}

impl std::fmt::Display for StepsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(text) => write!(
                f,
                "#walk cannot read {text}. Use n, e, s, w, u, and d, each with an optional count, like 3n2e."
            ),
            Self::TooMany => write!(f, "#walk takes at most {MAX_STEPS} steps."),
        }
    }
}

/// Read the steps of a `#walk` line, like `3n2e` or `2w u`.
pub(crate) fn parse_steps(text: &str) -> Result<Vec<Dir>, StepsError> {
    let chars: Vec<char> = text.chars().collect();
    let mut steps = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let mut count = 1;
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let digits: String = chars[start..i].iter().collect();
            // A count needs its direction right after it.
            let next = chars.get(i).copied().filter(|c| !c.is_whitespace());
            let Some(next) = next else {
                return Err(StepsError::Unreadable(digits));
            };
            if Dir::from_letter(next).is_none() {
                return Err(unreadable_at(&chars, i));
            }
            count = match digits.parse::<u32>() {
                Ok(n) if (1..=MAX_COUNT).contains(&n) => n,
                _ => return Err(StepsError::Unreadable(digits)),
            };
        }
        let Some(dir) = Dir::from_letter(chars[i]) else {
            return Err(unreadable_at(&chars, i));
        };
        for _ in 0..count {
            if steps.len() == MAX_STEPS {
                return Err(StepsError::TooMany);
            }
            steps.push(dir);
        }
        i += 1;
    }
    Ok(steps)
}

/// `steps` as a `#walk` string that [`parse_steps`] reads back, each run
/// of one direction as its count and letter, like `3n2e`.
pub(crate) fn steps_text(steps: &[Dir]) -> String {
    let mut text = String::new();
    for run in steps.chunk_by(|a, b| a == b) {
        for part in run.chunks(MAX_COUNT as usize) {
            if part.len() > 1 {
                text.push_str(&part.len().to_string());
            }
            text.push(part[0].letter());
        }
    }
    text
}

/// What Vosh cannot read at `at`: the run of letters around it, so
/// `north` reads as itself and the `x` of `3x` alone, or the one
/// character there when it is no letter.
fn unreadable_at(chars: &[char], at: usize) -> StepsError {
    if !chars[at].is_alphabetic() {
        return StepsError::Unreadable(chars[at].to_string());
    }
    let mut start = at;
    while start > 0 && chars[start - 1].is_alphabetic() {
        start -= 1;
    }
    let mut end = at;
    while end < chars.len() && chars[end].is_alphabetic() {
        end += 1;
    }
    StepsError::Unreadable(chars[start..end].iter().collect())
}

/// The words after `#walk` when `piece` is a `#walk` command, tokenized
/// the way the slash dispatcher reads a line, or None for any other
/// piece.
pub(crate) fn walk_args(piece: &str) -> Option<&str> {
    let rest = piece.trim_start().strip_prefix('#')?;
    let (cmd, args) = split_first_word(rest);
    (cmd == "walk").then_some(args)
}

/// The walk command `args` asks for, with `rest` after it, or the error
/// to echo. A walk that does not read drops the rest of its line, which
/// was meant for where it leads.
pub(crate) fn walk_command(args: &str, rest: Vec<ExpandStep>) -> Result<WalkCommand, StepsError> {
    let args = args.trim();
    if args.is_empty() {
        return Ok(WalkCommand::Status { rest });
    }
    if args.eq_ignore_ascii_case("stop") {
        return Ok(WalkCommand::Stop { key: false, rest });
    }
    let steps = parse_steps(args)?;
    Ok(WalkCommand::Start {
        plan: WalkPlan { steps, route: None },
        rest,
    })
}

/// The result of a `#walk` piece: the walk command, or the error echoed.
pub(crate) fn walk_result(args: &str, rest: Vec<ExpandStep>) -> InputResult {
    match walk_command(args, rest) {
        Ok(command) => InputResult {
            walk: Some(command),
            ..InputResult::empty()
        },
        Err(e) => InputResult::error(e),
    }
}

/// The rest of a slash line after `walk`, when its command is `#walk`,
/// even with a `;` straight after it as in `#walk;look`.
pub(super) fn slash_walk_args(rest: &str) -> Option<&str> {
    let args = rest.trim_start().strip_prefix("walk")?;
    let ends = args
        .chars()
        .next()
        .map_or(true, |c| c == ';' || c.is_whitespace());
    ends.then_some(args)
}

/// `#walk` typed, or from a timer, the tick or Lua. A line typed after
/// it with `;` waits for the walk, expanded the way any typed line is,
/// with its variables and aliases.
pub(super) fn slash_walk(profile: &mut Profile, c: &Connection, args: &str) -> InputResult {
    let args = c.var_view(profile).interpolate(args);
    let (head, tail) = split_at_separator(&args);
    let rest = match tail {
        Some(tail) => match profile
            .aliases
            .expand_line_full(tail, &c.plugin_aliases, c.stop_key)
        {
            Ok(steps) => steps,
            Err(ExpandError::RecursionLimit(depth)) => {
                return InputResult::error(format!("alias recursion limit hit ({depth})"));
            }
        },
        None => Vec::new(),
    };
    walk_result(head, rest)
}

/// `line` cut at its first command separator, a `;` or a line end that
/// no `\` escapes, as the alias engine splits a line.
fn split_at_separator(line: &str) -> (&str, Option<&str>) {
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            ';' | '\n' => return (&line[..i], Some(&line[i + c.len_utf8()..])),
            _ => {}
        }
    }
    (line, None)
}

#[cfg(test)]
mod tests;
