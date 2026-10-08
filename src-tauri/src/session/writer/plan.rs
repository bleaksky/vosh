//! What a job sends: your lines as they go out and as the game should
//! hold them, the stages each action runs, and the editor commands that
//! mend what the game holds into what you wrote.

use std::collections::VecDeque;

use super::game_text::{self, replace_line, stored, wire};
use super::kinds::Kind;
use super::payloads::{Action, WriteJob};

/// A command at the game's prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ask {
    /// `description`, for a read.
    Read,
    /// The kind's own board's `show`.
    Board,
    /// Another board's `show`, the index into [`BOARDS`].
    Other(usize),
    /// The board's `clear`, before a post you agreed to.
    ClearNote,
    To,
    Subject,
    Language,
    /// The read back once the editor closed.
    ReadBack,
    Post,
    Check,
    /// The board's `clear` after a refusal or a stop, which ends the job
    /// with what it already holds.
    ClearAfter,
}

/// What `.s` reads for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShowFor {
    Read,
    Start,
    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Stage {
    Ask(Ask),
    /// Open the editor on the kind's text.
    Open,
    /// Answer the pager until the editor waits, for an offer you took.
    Turn,
    Show(ShowFor),
    Clear,
    /// The planned lines.
    Send,
    /// Edits to lines the game holds, one on each `> `.
    Mend(VecDeque<String>),
    /// `@`, which leaves the editor.
    Close,
}

/// A line of your text: what goes out, and what the game should hold.
#[derive(Debug, Clone)]
pub(super) struct Planned {
    pub(super) wire: String,
    pub(super) held: String,
}

/// Your text as it goes out and as the game should hold it. A line that
/// starts with a dot, `@` or `!` goes as one space and is mended into
/// place with `.rl` (Description Editor board 6).
pub(super) fn plan(spec: &WriteJob) -> Vec<Planned> {
    if !matches!(spec.action, Action::Send | Action::Post | Action::Paste) {
        return Vec::new();
    }
    spec.lines
        .iter()
        .map(|line| {
            let line = wire(line);
            let held = stored(&line, spec.immortal);
            let wire = if spec.action != Action::Paste && game_text::needs_mend(&line) {
                " ".to_string()
            } else {
                line
            };
            Planned { wire, held }
        })
        .collect()
}

/// The stages of `spec`.
pub(super) fn stages(spec: &WriteJob) -> VecDeque<Stage> {
    let board = spec.kind.board().is_some();
    let mut s = VecDeque::new();
    match spec.action {
        Action::Read if spec.kind == Kind::Description => s.push_back(Stage::Ask(Ask::Read)),
        Action::Read if board => s.push_back(Stage::Ask(Ask::Board)),
        Action::Read => s.extend([Stage::Open, Stage::Show(ShowFor::Read), Stage::Close]),
        Action::Send => {
            s.extend([
                Stage::Open,
                Stage::Show(ShowFor::Start),
                Stage::Clear,
                Stage::Send,
                Stage::Show(ShowFor::Verify),
                Stage::Close,
            ]);
            if spec.kind.read_back().is_some() {
                s.push_back(Stage::Ask(Ask::ReadBack));
            }
        }
        Action::Post => {
            if spec.clear_first {
                s.push_back(Stage::Ask(Ask::ClearNote));
            } else {
                s.push_back(Stage::Ask(Ask::Board));
            }
            s.extend([Stage::Ask(Ask::To), Stage::Ask(Ask::Subject)]);
            if spec.kind.takes_language()
                && spec
                    .language
                    .as_deref()
                    .is_some_and(|l| !l.trim().is_empty())
            {
                s.push_back(Stage::Ask(Ask::Language));
            }
            s.extend([
                Stage::Open,
                Stage::Clear,
                Stage::Send,
                Stage::Show(ShowFor::Verify),
                Stage::Close,
                Stage::Ask(Ask::ReadBack),
                Stage::Ask(Ask::Post),
            ]);
        }
        Action::Check => s.push_back(Stage::Ask(Ask::Check)),
        Action::Clear => s.push_back(Stage::Ask(Ask::ClearNote)),
        Action::Paste => s.push_back(Stage::Send),
    }
    s
}

/// Two texts are the same line for line, the trailing spaces aside.
pub(super) fn same_text(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| game_text::same(x, y))
}

/// The first line, from 0, where `held` and `meant` part.
pub(super) fn first_difference(held: &[String], meant: &[String]) -> Option<usize> {
    (0..held.len().max(meant.len())).find(|at| match (held.get(*at), meant.get(*at)) {
        (Some(a), Some(b)) => !game_text::same(a, b),
        _ => true,
    })
}

/// The editor commands that turn `held` into `meant`, and the line from
/// which `meant` must go out again after them, appended. A line that
/// differs goes again with `.rl`, an extra one goes with `.d`, a missing
/// one at the end is sent again. `.rl` carries no code at a line's start,
/// so a line that opens with one is mended by deleting it and every line
/// after it and sending them again. Err names the line no mark fits.
pub(super) fn mend(
    held: &[String],
    meant: &[String],
) -> Result<(Vec<String>, Option<usize>), usize> {
    let common = held.len().min(meant.len());
    let differs = |at: usize| !game_text::same(&held[at], &meant[at]);
    if let Some(at) = (0..common).find(|at| differs(*at) && meant[*at].starts_with('`')) {
        let commands = (at..held.len()).map(|_| format!(".d {}", at + 1)).collect();
        return Ok((commands, Some(at)));
    }
    let mut commands = Vec::new();
    for at in (0..common).filter(|at| differs(*at)) {
        commands.push(replace_line(at + 1, &meant[at]).ok_or(at)?);
    }
    for at in (meant.len()..held.len()).rev() {
        commands.push(format!(".d {}", at + 1));
    }
    let from = (meant.len() > held.len()).then_some(held.len());
    Ok((commands, from))
}
