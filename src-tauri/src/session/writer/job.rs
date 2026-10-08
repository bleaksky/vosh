//! One thing the card asks of the game: read a text, send one through
//! the game's editor, post a note, send a text for its review, clear the
//! note the game holds, look for your note on a board's list, or pace a
//! paste into the editor you opened. A
//! job runs as a list of stages, each a command and what it waits for,
//! and stops at the first answer it does not expect, so it never leaves
//! a line where the game would take it as something else.
//!
//! A command at the game's prompt waits for the game's next prompt, the
//! GMCP prompt tick (`comm.c:1629`) once the text of its pulse is in, and
//! reads the lines it gathered as its answer. A command inside the editor waits for the editor's `> `
//! (`comm.c:1583`). A line of your text counts as taken when a `> `
//! comes alone, and after three pulses with none, since the game writes
//! one after anything it sends you while the editor is open, a say or a
//! weather line included. Never more than two wait unconfirmed, and the
//! `.s` that follows settles what landed (Description Editor Q5).

use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

use super::game_text::{
    after_header, editor_waits, listed_note, listing, opened_listing, pager_waits, shown_note,
    stored, uncoded, GameLine, ACCEPTED, BAD_DOT, BANNER, CLEARED, LANGUAGE_SET, NO_FORUM, NO_NOTE,
    OK, OTHER_BOARD, STAFF_ONLY, TOO_LONG,
};
use super::kinds::{Kind, BOARDS};
use super::payloads::{Action, Field, JobProgress, JobResult, Shown, Why, WriteJob};
use super::plan::{first_difference, mend, plan, same_text, stages, Ask, Planned, ShowFor, Stage};

/// The game's pulse, in milliseconds (`merc.h:407`).
const PULSE_MS: u64 = 250;

/// How long a line of your text waits for a `> ` alone before the card
/// counts it as taken, three pulses.
const CONFIRM: Duration = Duration::from_millis(3 * PULSE_MS);

/// How many lines may wait unconfirmed, about 160 of the 1,014 bytes the
/// game holds before it drops you (`comm.c:1442`).
const IN_FLIGHT: usize = 2;

/// How long any answer may take before the card gives up.
const BACKSTOP: Duration = Duration::from_secs(12);

/// How many pages of a long text the card turns before it gives up.
const PAGES: u8 = 40;

#[derive(Debug)]
enum Phase {
    /// Between stages.
    Idle,
    /// A command at the game's prompt, its answer gathering.
    Tick {
        ask: Option<Ask>,
        lines: Vec<GameLine>,
        since: Instant,
    },
    /// A command in the editor, waiting for its `> `.
    Editor {
        lines: Vec<GameLine>,
        since: Instant,
        banner: bool,
        cleared: bool,
        pages: u8,
    },
    /// The planned lines on their way.
    Sending {
        next: usize,
        flight: VecDeque<Instant>,
    },
}

/// One job under way. See the module notes.
#[derive(Debug)]
pub(crate) struct Job {
    spec: WriteJob,
    stages: VecDeque<Stage>,
    stage: Option<Stage>,
    phase: Phase,
    /// Your text as it goes out and as the game should hold it.
    planned: Vec<Planned>,
    /// The lines the card sends from `planned`, from this index on.
    from: usize,
    sent: usize,
    restore: Option<Vec<String>>,
    read: Option<Vec<String>>,
    beast: Option<String>,
    mends: u8,
    in_editor: bool,
    started_note: bool,
    post_sent: bool,
    /// What the job ends with once its last stage, such as a clear,
    /// is done.
    pending: Option<JobResult>,
    result: Option<JobResult>,
}

impl Job {
    /// A job for `spec`. `editor_open` says the game's editor is open on
    /// a text you opened yourself, which only a paste goes into.
    pub(crate) fn new(spec: WriteJob) -> Self {
        let planned = plan(&spec);
        let stages = stages(&spec);
        let in_editor = spec.action == Action::Paste;
        Self {
            spec,
            stages,
            stage: None,
            phase: Phase::Idle,
            planned,
            from: 0,
            sent: 0,
            restore: None,
            read: None,
            beast: None,
            mends: 0,
            in_editor,
            started_note: false,
            post_sent: false,
            pending: None,
            result: None,
        }
    }

    /// A job that takes the offer to open the card on the text the game
    /// just listed: it turns the pager when it waits, leaves the editor
    /// with `@`, and reads a board's note back, since the listing holds
    /// only the text.
    pub(crate) fn take(
        id: u64,
        kind: Kind,
        listed: Vec<String>,
        beast: Option<String>,
        pager: bool,
    ) -> Self {
        let spec = WriteJob {
            id,
            kind,
            action: Action::Read,
            lines: Vec::new(),
            to: String::new(),
            subject: String::new(),
            language: None,
            base: None,
            adopt: false,
            clear_first: false,
            name: None,
            immortal: false,
        };
        let mut job = Self::new(spec);
        let mut stages = VecDeque::from([Stage::Close]);
        if pager {
            stages.push_front(Stage::Turn);
        }
        if kind.board().is_some() {
            stages.push_back(Stage::Ask(Ask::Board));
        }
        job.stages = stages;
        job.in_editor = true;
        job.read = Some(listed);
        job.beast = beast;
        job
    }

    pub(crate) fn id(&self) -> u64 {
        self.spec.id
    }

    /// A paste runs in the editor you opened, and holds nothing back.
    pub(crate) fn holds(&self) -> bool {
        self.spec.action != Action::Paste
    }

    /// The game's editor is open for this job, so a line you type goes
    /// as `./` and your line.
    pub(crate) fn in_editor(&self) -> bool {
        self.in_editor
    }

    pub(crate) fn result(&self) -> Option<&JobResult> {
        self.result.as_ref()
    }

    pub(crate) fn progress(&self) -> JobProgress {
        let stage = match &self.stage {
            None => Shown::Waiting,
            Some(
                Stage::Ask(Ask::Read | Ask::Board | Ask::Other(_) | Ask::List)
                | Stage::Show(ShowFor::Read),
            ) => Shown::Reading,
            Some(Stage::Ask(Ask::To | Ask::Subject | Ask::Language | Ask::ClearNote)) => {
                Shown::Fields
            }
            Some(Stage::Ask(Ask::Post)) => Shown::Posting,
            Some(Stage::Ask(_) | Stage::Show(_) | Stage::Mend(_)) => Shown::Checking,
            Some(Stage::Open | Stage::Turn | Stage::Clear) => Shown::Opening,
            Some(Stage::Send) => Shown::Sending,
            Some(Stage::Close) => Shown::Closing,
        };
        JobProgress {
            id: self.spec.id,
            kind: self.spec.kind,
            action: self.spec.action,
            stage,
            sent: self.sent,
            total: self.planned.len(),
        }
    }

    /// When the job next needs a look without an event: a line of your
    /// text that waited for its `> `, or the backstop.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        match &self.phase {
            Phase::Idle => None,
            Phase::Tick { since, .. } | Phase::Editor { since, .. } => Some(*since + BACKSTOP),
            Phase::Sending { flight, .. } => flight.front().map(|sent| *sent + CONFIRM),
        }
    }

    /// Start the job at the game's prompt.
    pub(crate) fn start(&mut self, now: Instant, out: &mut Vec<String>) {
        self.advance(now, out);
    }

    /// A line the game sent.
    pub(crate) fn line(&mut self, line: &GameLine) {
        match &mut self.phase {
            Phase::Tick { lines, .. } | Phase::Editor { lines, .. } => lines.push(line.clone()),
            Phase::Sending { .. } | Phase::Idle => {}
        }
        match line.plain.as_str() {
            TOO_LONG if self.in_editor => {
                // The editor closed with the line it skipped.
                self.in_editor = false;
                self.fail_with(JobResult::TooLong { sent: self.sent });
            }
            BAD_DOT if self.in_editor => self.fail_with(JobResult::Failed {
                why: Why::BadDot,
                line: None,
            }),
            _ => {}
        }
        if let Phase::Editor {
            banner, cleared, ..
        } = &mut self.phase
        {
            if line.plain == BANNER[0] {
                *banner = true;
            }
            if line.plain == CLEARED {
                *cleared = true;
            }
        }
        if self.spec.kind == Kind::Beast {
            if let Some(name) = line
                .plain
                .strip_prefix("Remember, your beast is ")
                .and_then(|rest| rest.strip_suffix('.'))
            {
                self.beast = Some(name.to_string());
            }
        }
    }

    /// The end of a read: how many `> ` came alone in it, and the partial
    /// it ended on.
    pub(crate) fn read_end(
        &mut self,
        lone: usize,
        partial: &str,
        now: Instant,
        out: &mut Vec<String>,
    ) {
        match &mut self.phase {
            Phase::Sending { flight, .. } => {
                for _ in 0..lone {
                    if flight.pop_front().is_none() {
                        break;
                    }
                    self.sent += 1;
                }
                self.pump(now, out);
            }
            Phase::Editor { pages, since, .. } if pager_waits(partial) => {
                if *pages >= PAGES {
                    self.fail_with(JobResult::Failed {
                        why: Why::Silent,
                        line: None,
                    });
                    return;
                }
                *pages += 1;
                *since = now;
                out.push(String::new());
            }
            Phase::Editor { .. } if editor_waits(partial) => {
                self.editor_answered(partial, now, out);
            }
            // A long list waits on the pager, and the rest of it follows.
            Phase::Tick {
                ask: Some(Ask::List),
                since,
                ..
            } if pager_waits(partial) => {
                *since = now;
                out.push(String::new());
            }
            _ => {}
        }
    }

    /// The game's prompt tick, which comes only at the game's own prompt,
    /// once the text of its pulse is in. Returns true when a command still
    /// waits for its answer, which the next text brings.
    pub(crate) fn tick(&mut self, now: Instant, out: &mut Vec<String>) -> bool {
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Tick { ask, lines, since } => {
                let answered = ask.is_none() || lines.iter().any(|l| !l.plain.trim().is_empty());
                if !answered {
                    self.phase = Phase::Tick { ask, lines, since };
                    return true;
                }
                self.in_editor = false;
                match ask {
                    None => self.advance(now, out),
                    Some(ask) => self.answered(ask, &lines, now, out),
                }
                false
            }
            Phase::Editor { lines, .. } => {
                // The editor never opened, or it closed under the card.
                self.in_editor = false;
                if self.stage == Some(Stage::Open) {
                    let line = lines
                        .iter()
                        .rev()
                        .find(|l| !l.plain.trim().is_empty())
                        .map(|l| l.plain.trim().to_string())
                        .unwrap_or_default();
                    self.end(JobResult::Refused {
                        field: Field::Editor,
                        line,
                    });
                } else {
                    self.end(JobResult::Failed {
                        why: Why::Closed,
                        line: None,
                    });
                }
                false
            }
            Phase::Sending { .. } => {
                self.in_editor = false;
                self.end(JobResult::Failed {
                    why: Why::Closed,
                    line: None,
                });
                false
            }
            Phase::Idle => false,
        }
    }

    /// A look at the time: a line that waited long enough counts as
    /// taken, and a game that went quiet ends the job.
    pub(crate) fn poll(&mut self, now: Instant, out: &mut Vec<String>) {
        match &mut self.phase {
            Phase::Sending { flight, .. } => {
                while flight.front().is_some_and(|sent| now >= *sent + CONFIRM) {
                    flight.pop_front();
                    self.sent += 1;
                }
                self.pump(now, out);
            }
            Phase::Tick { since, .. } | Phase::Editor { since, .. } if now >= *since + BACKSTOP => {
                self.end(JobResult::Failed {
                    why: Why::Silent,
                    line: None,
                });
            }
            _ => {}
        }
    }

    /// Stop. Inside the editor the card leaves it with `@`, a note the
    /// card started goes with the board's `clear`, and once the post went
    /// out nothing stops.
    pub(crate) fn stop(&mut self, now: Instant, out: &mut Vec<String>) {
        if self.result.is_some() || self.post_sent {
            return;
        }
        let stopped = JobResult::Stopped { sent: self.sent };
        let board = self.spec.kind.board().is_some() && self.spec.action == Action::Post;
        let mut stages = VecDeque::new();
        if self.in_editor {
            stages.push_back(Stage::Close);
        }
        if board && (self.started_note || self.in_editor) {
            stages.push_back(Stage::Ask(Ask::ClearAfter));
        }
        if stages.is_empty() {
            self.end(stopped);
            return;
        }
        self.pending = Some(stopped);
        self.stages = stages;
        self.phase = Phase::Idle;
        self.advance(now, out);
    }

    /// The link dropped.
    pub(crate) fn dropped(&mut self) {
        if self.result.is_none() {
            self.in_editor = false;
            self.end(JobResult::Dropped {
                sent: self.sent,
                posted: self.post_sent,
            });
        }
    }

    /// Run the next stage.
    fn advance(&mut self, now: Instant, out: &mut Vec<String>) {
        if self.result.is_some() {
            return;
        }
        let Some(stage) = self.stages.pop_front() else {
            self.finish();
            return;
        };
        self.stage = Some(stage.clone());
        let editor = |since| Phase::Editor {
            lines: Vec::new(),
            since,
            banner: false,
            cleared: false,
            pages: 0,
        };
        match stage {
            Stage::Ask(ask) => {
                let Some(command) = self.command(ask) else {
                    self.advance(now, out);
                    return;
                };
                if ask == Ask::Post {
                    self.post_sent = true;
                }
                out.push(command);
                self.phase = Phase::Tick {
                    ask: Some(ask),
                    lines: Vec::new(),
                    since: now,
                };
            }
            Stage::Open => {
                out.push(self.spec.kind.opener());
                self.phase = editor(now);
            }
            Stage::Turn => {
                // The pager waits for Return, and the rest of the text
                // follows it.
                out.push(String::new());
                self.phase = Phase::Editor {
                    lines: Vec::new(),
                    since: now,
                    banner: true,
                    cleared: false,
                    pages: 1,
                };
            }
            Stage::Show(_) => {
                out.push(".s".to_string());
                self.phase = editor(now);
            }
            Stage::Clear => {
                out.push(".c".to_string());
                self.phase = editor(now);
            }
            Stage::Send => {
                self.phase = Phase::Sending {
                    next: self.from,
                    flight: VecDeque::new(),
                };
                self.pump(now, out);
            }
            Stage::Mend(mut commands) => match commands.pop_front() {
                Some(command) => {
                    out.push(command);
                    self.stages.push_front(Stage::Mend(commands));
                    self.phase = editor(now);
                }
                None => self.advance(now, out),
            },
            Stage::Close => {
                out.push("@".to_string());
                self.in_editor = false;
                self.phase = Phase::Tick {
                    ask: None,
                    lines: Vec::new(),
                    since: now,
                };
            }
        }
    }

    /// The command for `ask`, or None when the job has nothing to send
    /// for it.
    fn command(&self, ask: Ask) -> Option<String> {
        let board = self.spec.kind.board();
        let on = |what: &str| board.map(|b| format!("{b} {what}"));
        match ask {
            Ask::Read => Some("description".to_string()),
            Ask::Board => on("show"),
            Ask::Other(at) => BOARDS.get(at)?.board().map(|b| format!("{b} show")),
            Ask::ClearNote | Ask::ClearAfter => on("clear"),
            Ask::To => {
                let to = if self.spec.kind.to_immortal() {
                    "immortal"
                } else {
                    self.spec.to.trim()
                };
                on(&format!("to {to}"))
            }
            Ask::Subject => on(&format!("subject {}", self.spec.subject.trim())),
            Ask::Language => {
                let language = self.spec.language.as_deref()?.trim();
                on(&format!("language {language}"))
            }
            Ask::ReadBack => match self.spec.kind.read_back() {
                Some(command) => Some(command.to_string()),
                None => on("show"),
            },
            Ask::Post => on("post"),
            Ask::Check => self.spec.kind.check().map(str::to_string),
            Ask::List => on("list"),
        }
    }

    /// The game answered `ask` with `lines` and its prompt.
    fn answered(&mut self, ask: Ask, lines: &[GameLine], now: Instant, out: &mut Vec<String>) {
        let has = |text: &str| lines.iter().any(|l| l.plain.trim() == text);
        let first = || {
            lines
                .iter()
                .map(|l| l.plain.trim())
                .find(|l| !l.is_empty())
                .unwrap_or_default()
                .to_string()
        };
        let name = self.spec.name.clone();
        match ask {
            Ask::Read => match after_header(lines, &["Your description is:"]) {
                Some(text) => self.end(JobResult::Read {
                    lines: text,
                    beast: None,
                    note: None,
                }),
                None => self.end(JobResult::Failed {
                    why: Why::Unread,
                    line: None,
                }),
            },
            Ask::Board => {
                if has(NO_NOTE) {
                    if self.spec.action == Action::Read {
                        let lines = self.read.take().unwrap_or_default();
                        self.end(JobResult::Read {
                            lines,
                            beast: None,
                            note: None,
                        });
                    } else {
                        self.advance(now, out);
                    }
                } else if has(OTHER_BOARD) {
                    let first = BOARDS
                        .iter()
                        .position(|b| *b != self.spec.kind)
                        .unwrap_or(0);
                    self.stages.push_front(Stage::Ask(Ask::Other(first)));
                    self.advance(now, out);
                } else if let Some(note) = shown_note(lines, name.as_deref()) {
                    if self.spec.action == Action::Read {
                        self.end(JobResult::Read {
                            lines: note.lines.clone(),
                            beast: None,
                            note: Some(note),
                        });
                    } else if self.spec.adopt {
                        self.advance(now, out);
                    } else {
                        self.end(JobResult::SameNote { note });
                    }
                } else {
                    self.end(JobResult::Failed {
                        why: Why::Unread,
                        line: None,
                    });
                }
            }
            Ask::Other(at) => {
                let board = BOARDS.get(at).copied();
                match shown_note(lines, name.as_deref()) {
                    Some(note) => self.end(JobResult::OtherNote {
                        board,
                        note: Some(note),
                    }),
                    None => {
                        let next = (at + 1..BOARDS.len()).find(|n| BOARDS[*n] != self.spec.kind);
                        match next {
                            Some(next) => {
                                self.stages.push_front(Stage::Ask(Ask::Other(next)));
                                self.advance(now, out);
                            }
                            None => self.end(JobResult::OtherNote {
                                board: None,
                                note: None,
                            }),
                        }
                    }
                }
            }
            Ask::ClearNote => self.advance(now, out),
            Ask::To => {
                if has(OK) {
                    self.started_note = true;
                    self.advance(now, out);
                } else {
                    self.end(JobResult::Refused {
                        field: Field::To,
                        line: first(),
                    });
                }
            }
            Ask::Subject | Ask::Language => {
                let took = if ask == Ask::Subject {
                    has(OK)
                } else {
                    lines.iter().any(|l| {
                        LANGUAGE_SET
                            .iter()
                            .any(|set| l.plain.trim().starts_with(set))
                    })
                };
                if took {
                    self.advance(now, out);
                } else {
                    let field = if ask == Ask::Subject {
                        Field::Subject
                    } else {
                        Field::Language
                    };
                    self.clear_then(
                        JobResult::Refused {
                            field,
                            line: first(),
                        },
                        now,
                        out,
                    );
                }
            }
            Ask::ReadBack => self.read_back(lines, now, out),
            Ask::Post => {
                if has(ACCEPTED) {
                    self.end(JobResult::Posted {
                        forum: true,
                        vote: true,
                    });
                } else if has(OK) {
                    self.end(JobResult::Posted {
                        forum: !has(NO_FORUM),
                        vote: false,
                    });
                } else {
                    self.post_sent = false;
                    self.clear_then(
                        JobResult::Refused {
                            field: Field::Post,
                            line: first(),
                        },
                        now,
                        out,
                    );
                }
            }
            Ask::Check => self.end(JobResult::Checked {
                lines: lines
                    .iter()
                    .map(|l| l.plain.trim_end().to_string())
                    .filter(|l| !l.is_empty())
                    .collect(),
            }),
            Ask::ClearAfter => self.finish(),
            Ask::List => {
                let subject = uncoded(&stored(self.spec.subject.trim(), self.spec.immortal));
                let result = match name {
                    _ if STAFF_ONLY.iter().any(|line| has(line)) => JobResult::CantTell,
                    None => JobResult::CantTell,
                    Some(name) => match listed_note(lines, &name, &subject) {
                        Some(number) => JobResult::Found { number },
                        None => JobResult::NotFound,
                    },
                };
                self.end(result);
            }
        }
    }

    /// The editor answered the command in the stage under way with its
    /// `> `.
    fn editor_answered(&mut self, partial: &str, now: Instant, out: &mut Vec<String>) {
        let Phase::Editor {
            lines,
            since,
            banner,
            cleared,
            pages,
        } = std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            return;
        };
        // A `> ` for a line the card counted as taken can come after the
        // next command went, and `.s` always lists a number, so a `> `
        // with no listing before it is not the answer to `.s`.
        let listed =
            !listing(&lines).is_empty() || partial.replace('\r', "").trim_start().starts_with('1');
        if matches!(self.stage, Some(Stage::Show(_))) && !listed {
            self.phase = Phase::Editor {
                lines,
                since,
                banner,
                cleared,
                pages,
            };
            return;
        }
        match self.stage.clone() {
            Some(Stage::Open) => {
                if !banner {
                    // A `> ` from an editor the card did not open.
                    self.end(JobResult::Busy);
                    return;
                }
                self.in_editor = true;
                if self.read.is_none() && self.spec.action == Action::Read {
                    self.read = Some(opened_listing(&lines));
                }
                self.advance(now, out);
            }
            Some(Stage::Turn) => {
                // What the pager showed joins the listing.
                if let Some(read) = &mut self.read {
                    read.extend(opened_listing(&lines));
                }
                self.advance(now, out);
            }
            Some(Stage::Show(why)) => {
                let held = listing(&lines);
                self.shown(why, held, now, out);
            }
            Some(Stage::Clear) => {
                if cleared {
                    self.advance(now, out);
                } else {
                    self.phase = Phase::Editor {
                        lines,
                        since,
                        banner,
                        cleared,
                        pages,
                    };
                }
            }
            _ => self.advance(now, out),
        }
    }

    /// `.s` listed `held`.
    fn shown(&mut self, why: ShowFor, held: Vec<String>, now: Instant, out: &mut Vec<String>) {
        match why {
            ShowFor::Read => {
                self.read = Some(held);
                self.advance(now, out);
            }
            ShowFor::Start => {
                let changed = self.spec.base.as_ref().is_some_and(|base| {
                    let base: Vec<String> =
                        base.iter().map(|l| stored(l, self.spec.immortal)).collect();
                    !same_text(&held, &base)
                });
                if changed {
                    self.pending = Some(JobResult::Changed { lines: held });
                    self.stages = VecDeque::from([Stage::Close]);
                } else {
                    self.restore = Some(held);
                }
                self.advance(now, out);
            }
            ShowFor::Verify => {
                let meant: Vec<String> = self.planned.iter().map(|p| p.held.clone()).collect();
                if same_text(&held, &meant) {
                    self.read = Some(held);
                    self.advance(now, out);
                    return;
                }
                if self.mends >= 2 {
                    let line = first_difference(&held, &meant).map(|at| at + 1);
                    self.fail_in_editor(
                        JobResult::Failed {
                            why: Why::Mend,
                            line,
                        },
                        now,
                        out,
                    );
                    return;
                }
                self.mends += 1;
                match mend(&held, &meant) {
                    Ok((commands, from)) => {
                        let mut stages = VecDeque::new();
                        stages.push_back(Stage::Mend(commands.into()));
                        if let Some(from) = from {
                            self.from = from;
                            stages.push_back(Stage::Send);
                        }
                        stages.push_back(Stage::Show(ShowFor::Verify));
                        stages.extend(self.stages.drain(..));
                        self.stages = stages;
                        self.advance(now, out);
                    }
                    Err(line) => self.fail_in_editor(
                        JobResult::Failed {
                            why: Why::NoMark,
                            line: Some(line + 1),
                        },
                        now,
                        out,
                    ),
                }
            }
        }
    }

    /// The read back once the editor closed: your description or beast
    /// as the game holds it, or the note it will post.
    fn read_back(&mut self, lines: &[GameLine], now: Instant, out: &mut Vec<String>) {
        if self.spec.kind.board().is_none() {
            let headers: &[&str] = match self.spec.kind {
                Kind::Beast => &["Your beast's description is:", "Your beast description is:"],
                _ => &["Your description is:"],
            };
            let lines = after_header(lines, headers)
                .or_else(|| self.read.clone())
                .unwrap_or_default();
            self.end(JobResult::Sent {
                lines,
                restore: self.restore.clone(),
            });
            return;
        }
        let meant: Vec<String> = self.planned.iter().map(|p| p.held.clone()).collect();
        let note = shown_note(lines, self.spec.name.as_deref());
        let matches = note.as_ref().is_some_and(|note| {
            let to = if self.spec.kind.to_immortal() {
                "Immortal".to_string()
            } else {
                stored(self.spec.to.trim(), self.spec.immortal)
            };
            same_text(&note.lines, &meant)
                && note.subject == stored(self.spec.subject.trim(), self.spec.immortal)
                && note.to == to
        });
        if matches {
            self.advance(now, out);
        } else {
            self.clear_then(
                JobResult::Failed {
                    why: Why::Differs,
                    line: None,
                },
                now,
                out,
            );
        }
    }

    /// Send the next lines of your text while fewer than two wait, and
    /// move on once the game took them all.
    fn pump(&mut self, now: Instant, out: &mut Vec<String>) {
        let Phase::Sending { next, flight } = &mut self.phase else {
            return;
        };
        while flight.len() < IN_FLIGHT && *next < self.planned.len() {
            out.push(self.planned[*next].wire.clone());
            *next += 1;
            flight.push_back(now);
        }
        if *next >= self.planned.len() && flight.is_empty() {
            self.phase = Phase::Idle;
            self.sent = self.planned.len();
            self.advance(now, out);
        }
    }

    /// End with `result` once the board's `clear` frees the note the card
    /// put there.
    fn clear_then(&mut self, result: JobResult, now: Instant, out: &mut Vec<String>) {
        if self.spec.kind.board().is_some() && self.spec.action == Action::Post {
            self.pending = Some(result);
            self.stages = VecDeque::from([Stage::Ask(Ask::ClearAfter)]);
            self.phase = Phase::Idle;
            self.advance(now, out);
        } else {
            self.end(result);
        }
    }

    /// Leave the editor, then end with `result`, clearing a note the card
    /// put there.
    fn fail_in_editor(&mut self, result: JobResult, now: Instant, out: &mut Vec<String>) {
        self.pending = Some(result);
        let mut stages = VecDeque::from([Stage::Close]);
        if self.spec.kind.board().is_some() && self.spec.action == Action::Post {
            stages.push_back(Stage::Ask(Ask::ClearAfter));
        }
        self.stages = stages;
        self.phase = Phase::Idle;
        self.advance(now, out);
    }

    /// A line the editor sent ends the job at once: it closed, or it
    /// says the card got a command wrong.
    fn fail_with(&mut self, result: JobResult) {
        self.phase = Phase::Idle;
        self.end(result);
    }

    /// Every stage ran.
    fn finish(&mut self) {
        let result = self
            .pending
            .take()
            .unwrap_or_else(|| match self.spec.action {
                Action::Read => JobResult::Read {
                    lines: self.read.clone().unwrap_or_default(),
                    beast: self.beast.clone(),
                    note: None,
                },
                Action::Send => JobResult::Sent {
                    lines: self.read.clone().unwrap_or_default(),
                    restore: self.restore.clone(),
                },
                Action::Paste => JobResult::Pasted,
                // A find on a text no board holds sent nothing.
                Action::Find => JobResult::CantTell,
                // A post ends at its answer and a check at its lines, so
                // only a clear ends here.
                Action::Clear | Action::Post | Action::Check => JobResult::Cleared,
            });
        self.end(result);
    }

    fn end(&mut self, result: JobResult) {
        self.phase = Phase::Idle;
        self.stage = None;
        self.stages.clear();
        if self.result.is_none() {
            self.result = Some(result);
        }
    }
}
