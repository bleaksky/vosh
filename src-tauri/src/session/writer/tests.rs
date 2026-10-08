//! The writer against the game's own answers, line by line, as
//! `string_add` and `parse_note` give them.

use std::time::Duration;

use tokio::time::Instant;

use super::game_text::GameLine;
use super::kinds::Kind;
use super::payloads::{Action, Field, JobResult, Why, WriteJob};
use super::{Game, Writer, WriterCommand};

/// A writer and the game it talks to, with the lines it sent.
struct Table {
    writer: Writer,
    now: Instant,
    /// Lines the session sent.
    out: u64,
    sent: Vec<String>,
}

const LISTED: [&str; 2] = [
    "This tall elf stands with a straight back,",
    "her hair bound in silver.",
];

impl Table {
    fn new() -> Self {
        let mut table = Self {
            writer: Writer::default(),
            now: Instant::now(),
            out: 0,
            sent: Vec::new(),
        };
        table.tick();
        table
    }

    fn take(&mut self, send: Vec<String>) -> Vec<String> {
        self.out += send.len() as u64;
        self.sent.extend(send.iter().cloned());
        self.writer.settle();
        send
    }

    fn run(&mut self, command: WriterCommand) -> Vec<String> {
        let send = self.writer.command(command, self.out, self.now);
        self.take(send)
    }

    /// The game prints `lines` and ends the read on `partial`.
    fn game(&mut self, lines: &[&str], partial: &str) -> Vec<String> {
        for line in lines {
            self.writer
                .line(&GameLine::new(line, line.as_bytes()), self.out);
        }
        let mut text = lines.iter().fold(String::new(), |mut text, line| {
            text.push_str(line);
            text.push('\n');
            text
        });
        text.push_str(partial);
        let send = self.writer.read_end(&text, partial, self.out, self.now);
        self.take(send)
    }

    /// The editor's `> ` alone, for a line it took.
    fn took(&mut self) -> Vec<String> {
        self.game(&[], "> ")
    }

    /// The game's prompt and its tick.
    fn tick(&mut self) -> Vec<String> {
        let send = self.writer.tick(self.now);
        self.take(send)
    }

    /// The game's lines, its prompt and its tick.
    fn answer(&mut self, lines: &[&str]) -> Vec<String> {
        let mut all: Vec<&str> = lines.to_vec();
        all.push("");
        let send = self.game(&all, "<1020hp 800m 930mv> ");
        assert_eq!(send, Vec::<String>::new(), "sent before the prompt");
        self.tick()
    }

    fn typed(&mut self, line: &str) -> Vec<u8> {
        self.out += 1;
        self.writer
            .typed(format!("{line}\r\n").as_bytes(), self.out)
    }

    fn later(&mut self, by: Duration) -> Vec<String> {
        self.now += by;
        let send = self.writer.poll(self.now);
        self.take(send)
    }

    fn done(&self) -> Option<JobResult> {
        self.writer.state(0).done.map(|d| d.result)
    }

    /// The editor opens on `listed` after `opener`'s banner.
    fn opens(&mut self, before: &[&str], listed: &[&str]) -> Vec<String> {
        let mut lines: Vec<&str> = before.to_vec();
        lines.extend(super::game_text::BANNER);
        lines.extend(listed);
        self.game(&lines, "> ")
    }

    /// `.s` lists `lines`.
    fn lists(&mut self, lines: &[&str]) -> Vec<String> {
        let numbered: Vec<String> = lines
            .iter()
            .enumerate()
            .map(|(n, l)| format!("{:>2} {l}", n + 1))
            .collect();
        let refs: Vec<&str> = numbered.iter().map(String::as_str).collect();
        if refs.is_empty() {
            return self.game(&[], " 1 > ");
        }
        self.game(&refs, "> ")
    }
}

fn job(id: u64, kind: Kind, action: Action, lines: &[&str]) -> WriteJob {
    WriteJob {
        id,
        kind,
        action,
        lines: lines.iter().copied().map(String::from).collect(),
        to: String::new(),
        subject: String::new(),
        language: None,
        base: None,
        adopt: false,
        clear_first: false,
        name: Some("Orla".into()),
        immortal: false,
    }
}

const TEXT: [&str; 3] = [
    "This tall elf stands with a straight back,",
    "",
    "her silver hair bound behind her.",
];

/// Send TEXT through the editor up to the `.s` that checks it.
fn send_to_check(t: &mut Table, spec: WriteJob) {
    assert_eq!(t.run(WriterCommand::Start(spec)), vec!["description edit"]);
    assert_eq!(t.opens(&[], &LISTED), vec![".s"]);
    assert_eq!(t.lists(&LISTED), vec![".c"]);
    assert_eq!(t.game(&["String cleared."], "> "), vec![TEXT[0], " "]);
    assert_eq!(t.took(), vec![TEXT[2]]);
    assert_eq!(t.took(), Vec::<String>::new());
    assert_eq!(t.took(), vec![".s"]);
}

#[test]
fn sends_a_description_and_reads_it_back() {
    let mut t = Table::new();
    send_to_check(&mut t, job(1, Kind::Description, Action::Send, &TEXT));
    assert!(t.writer.holds());
    assert_eq!(t.lists(&TEXT), vec!["@"]);
    assert_eq!(t.tick(), vec!["description"]);
    assert_eq!(
        t.answer(&["Your description is:", TEXT[0], " ", TEXT[2]]),
        Vec::<String>::new()
    );
    assert_eq!(
        t.done(),
        Some(JobResult::Sent {
            lines: vec![TEXT[0].into(), String::new(), TEXT[2].into()],
            restore: Some(LISTED.iter().copied().map(String::from).collect()),
        })
    );
    assert!(!t.writer.holds());
    assert_eq!(t.writer.state(0).job, None);
}

#[test]
fn counts_a_line_taken_after_three_pulses_with_no_prompt_alone() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &TEXT,
    )));
    t.opens(&[], &LISTED);
    t.lists(&LISTED);
    t.game(&["String cleared."], "> ");
    // A say comes with its own > in the same pulse.
    assert_eq!(
        t.game(&["\rMaren says 'hello'"], "> "),
        Vec::<String>::new()
    );
    assert_eq!(t.later(Duration::from_millis(500)), Vec::<String>::new());
    assert_eq!(t.later(Duration::from_millis(300)), vec![TEXT[2]]);
}

#[test]
fn stops_to_ask_when_the_game_holds_another_text() {
    let mut t = Table::new();
    let mut spec = job(1, Kind::Description, Action::Send, &TEXT);
    spec.base = Some(vec!["An older text.".into()]);
    t.run(WriterCommand::Start(spec));
    assert_eq!(t.opens(&[], &LISTED), vec![".s"]);
    assert_eq!(t.lists(&LISTED), vec!["@"]);
    assert_eq!(t.tick(), Vec::<String>::new());
    assert_eq!(
        t.done(),
        Some(JobResult::Changed {
            lines: LISTED.iter().copied().map(String::from).collect()
        })
    );
}

#[test]
fn mends_a_line_that_differs_and_then_leaves() {
    let mut t = Table::new();
    send_to_check(&mut t, job(1, Kind::Description, Action::Send, &TEXT));
    assert_eq!(
        t.lists(&[TEXT[0], "", "Maren says 'hello'"]),
        vec![".rl 3 'her silver hair bound behind her.'"]
    );
    assert_eq!(t.game(&["Line replaced."], "> "), vec![".s"]);
    assert_eq!(t.lists(&TEXT), vec!["@"]);
}

#[test]
fn deletes_an_extra_line_and_sends_a_missing_one() {
    let mut t = Table::new();
    send_to_check(&mut t, job(1, Kind::Description, Action::Send, &TEXT));
    assert_eq!(t.lists(&[TEXT[0], "", TEXT[2], "stray"]), vec![".d 4"]);
    assert_eq!(t.game(&["Line 4 deleted."], "> "), vec![".s"]);
    assert_eq!(t.lists(&[TEXT[0], ""]), vec![TEXT[2]]);
    assert_eq!(t.took(), vec![".s"]);
}

#[test]
fn puts_a_line_that_starts_with_a_dot_in_with_rl() {
    let mut t = Table::new();
    let text = ["...the nightgaunt waits.", "It does not move."];
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &text,
    )));
    t.opens(&[], &[]);
    assert_eq!(t.lists(&[]), vec![".c"]);
    assert_eq!(t.game(&["String cleared."], "> "), vec![" ", text[1]]);
    t.took();
    assert_eq!(t.took(), vec![".s"]);
    assert_eq!(
        t.lists(&["", text[1]]),
        vec![".rl 1 '...the nightgaunt waits.'"]
    );
}

#[test]
fn mends_a_line_that_opens_with_a_code_by_sending_it_again() {
    let mut t = Table::new();
    let text = ["A plain line.", "`#A bold yellow line."];
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &text,
    )));
    t.opens(&[], &[]);
    t.lists(&[]);
    t.game(&["String cleared."], "> ");
    t.took();
    assert_eq!(t.took(), vec![".s"]);
    assert_eq!(t.lists(&[text[0], "A bold yellow line."]), vec![".d 2"]);
    assert_eq!(t.game(&["Line 2 deleted."], "> "), vec![text[1]]);
    assert_eq!(t.took(), vec![".s"]);
    assert_eq!(t.lists(&text), vec!["@"]);
}

#[test]
fn gives_up_after_two_mends() {
    let mut t = Table::new();
    send_to_check(&mut t, job(1, Kind::Description, Action::Send, &TEXT));
    let wrong = [TEXT[0], "", "wrong"];
    t.lists(&wrong);
    t.game(&["Line replaced."], "> ");
    t.lists(&wrong);
    t.game(&["Line replaced."], "> ");
    assert_eq!(t.lists(&wrong), vec!["@"]);
    t.tick();
    assert_eq!(
        t.done(),
        Some(JobResult::Failed {
            why: Why::Mend,
            line: Some(3)
        })
    );
}

#[test]
fn ends_when_the_text_runs_past_what_the_editor_holds() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &TEXT,
    )));
    t.opens(&[], &[]);
    t.lists(&[]);
    t.game(&["String cleared."], "> ");
    t.game(&["String too long, last line skipped."], "");
    assert_eq!(t.done(), Some(JobResult::TooLong { sent: 0 }));
    assert!(!t.writer.holds());
}

#[test]
fn turns_the_pager_while_the_editor_lists_a_long_text() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &TEXT,
    )));
    let mut lines: Vec<&str> = super::game_text::BANNER.to_vec();
    lines.push(LISTED[0]);
    assert_eq!(t.game(&lines, "\r[Hit Return to continue]\r"), vec![""]);
    assert_eq!(t.game(&[LISTED[1]], "> "), vec![".s"]);
}

#[test]
fn reads_your_description_without_the_editor() {
    let mut t = Table::new();
    assert_eq!(
        t.run(WriterCommand::Start(job(
            1,
            Kind::Description,
            Action::Read,
            &[]
        ))),
        vec!["description"]
    );
    t.answer(&["Your description is:", LISTED[0], LISTED[1]]);
    assert_eq!(
        t.done(),
        Some(JobResult::Read {
            lines: LISTED.iter().copied().map(String::from).collect(),
            beast: None,
            note: None,
        })
    );
}

#[test]
fn reads_a_beast_through_its_editor_for_its_name() {
    let mut t = Table::new();
    assert_eq!(
        t.run(WriterCommand::Start(job(1, Kind::Beast, Action::Read, &[]))),
        vec!["beastdesc edit"]
    );
    assert_eq!(
        t.opens(&["Remember, your beast is wolf."], &[""]),
        vec![".s"]
    );
    assert_eq!(t.lists(&[]), vec!["@"]);
    t.tick();
    assert_eq!(
        t.done(),
        Some(JobResult::Read {
            lines: vec![],
            beast: Some("wolf".into()),
            note: None,
        })
    );
}

#[test]
fn waits_for_the_games_prompt_and_turns_away_from_an_editor() {
    let mut t = Table::new();
    t.game(&[], "> ");
    assert_eq!(t.writer.state(0).game, Game::Editor);
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Read,
        &[],
    )));
    assert_eq!(t.done(), Some(JobResult::Busy));
    t.tick();
    t.game(&["Long text"], "\r[Hit Return to continue]\r");
    assert_eq!(
        t.run(WriterCommand::Start(job(
            2,
            Kind::Description,
            Action::Read,
            &[]
        ))),
        vec![""]
    );
    assert_eq!(t.tick(), vec!["description"]);
}

fn note(id: u64, action: Action) -> WriteJob {
    let mut spec = job(id, Kind::Note, action, &TEXT);
    spec.to = "all".into();
    spec.subject = "The Great Milieu".into();
    spec
}

#[test]
fn posts_a_note_once_the_game_holds_it_as_written() {
    let mut t = Table::new();
    assert_eq!(
        t.run(WriterCommand::Start(note(1, Action::Post))),
        vec!["note show"]
    );
    assert_eq!(
        t.answer(&["You have no note in progress."]),
        vec!["note to all"]
    );
    assert_eq!(t.answer(&["Ok."]), vec!["note subject The Great Milieu"]);
    assert_eq!(t.answer(&["Ok."]), vec!["note edit"]);
    assert_eq!(t.opens(&[], &[""]), vec![".c"]);
    assert_eq!(t.game(&["String cleared."], "> "), vec![TEXT[0], " "]);
    t.took();
    t.took();
    assert_eq!(t.took(), vec![".s"]);
    assert_eq!(t.lists(&TEXT), vec!["@"]);
    assert_eq!(t.tick(), vec!["note show"]);
    assert_eq!(
        t.answer(&["Orla: The Great Milieu", "To: all", TEXT[0], " ", TEXT[2]]),
        vec!["note post"]
    );
    t.answer(&["Ok."]);
    assert_eq!(
        t.done(),
        Some(JobResult::Posted {
            forum: true,
            vote: false
        })
    );
}

#[test]
fn stops_at_a_refused_to_and_clears_after_a_refused_subject() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(note(1, Action::Post)));
    t.answer(&["You have no note in progress."]);
    t.answer(&["You can only post notes to your own class."]);
    assert_eq!(
        t.done(),
        Some(JobResult::Refused {
            field: Field::To,
            line: "You can only post notes to your own class.".into()
        })
    );
    let mut t = Table::new();
    let mut spec = note(2, Action::Post);
    spec.language = Some("Elvish".into());
    t.run(WriterCommand::Start(spec));
    t.answer(&["You have no note in progress."]);
    t.answer(&["Ok."]);
    assert_eq!(t.answer(&["Ok."]), vec!["note language Elvish"]);
    assert_eq!(
        t.answer(&["You don't know enough Elvish to write a note in it."]),
        vec!["note clear"]
    );
    t.answer(&["Ok."]);
    assert_eq!(
        t.done(),
        Some(JobResult::Refused {
            field: Field::Language,
            line: "You don't know enough Elvish to write a note in it.".into()
        })
    );
}

#[test]
fn finds_a_note_started_on_another_board() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(note(1, Action::Post)));
    assert_eq!(
        t.answer(&["You aren't working on that kind of note."]),
        vec!["journal show"]
    );
    assert_eq!(
        t.answer(&["You aren't working on that kind of note."]),
        vec!["application show"]
    );
    assert_eq!(
        t.answer(&["You aren't working on that kind of note."]),
        vec!["idea show"]
    );
    t.answer(&["Orla: A better map", "To: Immortal", "Draw the roads."]);
    match t.done() {
        Some(JobResult::OtherNote {
            board: Some(Kind::Idea),
            note: Some(note),
        }) => assert_eq!(note.subject, "A better map"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn clears_its_copy_when_the_game_turns_a_post_down() {
    let mut t = Table::new();
    let mut spec = note(1, Action::Post);
    spec.kind = Kind::Application;
    spec.to = "immortal".into();
    spec.subject = "Psi Application".into();
    t.run(WriterCommand::Start(spec));
    t.answer(&["You have no note in progress."]);
    t.answer(&["Ok."]);
    t.answer(&["Ok."]);
    t.opens(&[], &[""]);
    t.game(&["String cleared."], "> ");
    t.took();
    t.took();
    t.took();
    t.lists(&TEXT);
    t.tick();
    assert_eq!(
        t.answer(&[
            "Orla: Psi Application",
            "To: immortal",
            TEXT[0],
            " ",
            TEXT[2]
        ]),
        vec!["application post"]
    );
    let refusal = "You may only make this application between ranks of 50 and 50.";
    assert_eq!(t.answer(&[refusal]), vec!["application clear"]);
    t.answer(&["Ok."]);
    assert_eq!(
        t.done(),
        Some(JobResult::Refused {
            field: Field::Post,
            line: refusal.into()
        })
    );
}

#[test]
fn stop_leaves_the_editor_and_clears_the_note_it_started() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(note(1, Action::Post)));
    t.answer(&["You have no note in progress."]);
    t.answer(&["Ok."]);
    t.answer(&["Ok."]);
    t.opens(&[], &[""]);
    t.game(&["String cleared."], "> ");
    assert_eq!(t.run(WriterCommand::Stop), vec!["@"]);
    assert_eq!(t.tick(), vec!["note clear"]);
    t.answer(&["Ok."]);
    assert_eq!(t.done(), Some(JobResult::Stopped { sent: 0 }));
}

#[test]
fn sends_your_typed_line_as_a_game_command_while_the_editor_is_open() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &TEXT,
    )));
    assert_eq!(t.typed("look"), b"look\r\n");
    t.opens(&[], &LISTED);
    assert_eq!(t.typed("flee"), b"./ flee\r\n");
}

#[test]
fn offers_the_card_after_you_open_the_editor_and_opens_on_its_listing() {
    let mut t = Table::new();
    t.typed("desc edit");
    assert_eq!(t.opens(&[], &LISTED), Vec::<String>::new());
    let state = t.writer.state(0);
    assert_eq!(state.editor, Some(Kind::Description));
    let offer = state.offer.expect("an offer");
    assert_eq!(offer.kind, Kind::Description);
    assert_eq!(t.run(WriterCommand::Take { id: offer.id }), vec!["@"]);
    t.tick();
    assert_eq!(
        t.done(),
        Some(JobResult::Read {
            lines: LISTED.iter().copied().map(String::from).collect(),
            beast: None,
            note: None,
        })
    );
}

#[test]
fn takes_the_offer_back_when_anything_else_went_out() {
    let mut t = Table::new();
    t.typed("history edit");
    t.out += 1;
    t.game(&["Editing your HISTORY.."], "");
    t.opens(&[], &[""]);
    assert_eq!(t.writer.state(0).offer, None);
    // The game named the text, so you still type into it raw.
    assert_eq!(t.writer.state(0).editor, Some(Kind::History));
    t.tick();
    assert_eq!(t.writer.state(0).editor, None);
    // A history needs its own line before the banner.
    t.typed("history edit");
    t.opens(&[], &[""]);
    assert_eq!(t.writer.state(0).editor, None);
}

#[test]
fn reads_a_notes_fields_once_you_take_the_offer() {
    let mut t = Table::new();
    t.typed("note edit");
    t.opens(&[], &[""]);
    let offer = t.writer.state(0).offer.expect("an offer");
    t.typed("look");
    assert_eq!(t.writer.state(0).offer, None);
    assert_eq!(
        t.run(WriterCommand::Take { id: offer.id }),
        Vec::<String>::new()
    );
    assert_eq!(t.done(), Some(JobResult::OfferGone));
    t.tick();
    t.typed("note edit");
    t.opens(&[], &[""]);
    let offer = t.writer.state(0).offer.expect("an offer");
    assert_eq!(t.run(WriterCommand::Take { id: offer.id }), vec!["@"]);
    assert_eq!(t.tick(), vec!["note show"]);
    t.answer(&["Tolliver: About the gate", "To: Maren"]);
    match t.done() {
        Some(JobResult::Read {
            note: Some(note), ..
        }) => {
            assert_eq!(note.to, "Maren");
            assert_eq!(note.subject, "About the gate");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_drop_ends_the_job_with_what_the_game_took() {
    let mut t = Table::new();
    t.run(WriterCommand::Start(job(
        1,
        Kind::Description,
        Action::Send,
        &TEXT,
    )));
    t.opens(&[], &LISTED);
    t.lists(&LISTED);
    t.game(&["String cleared."], "> ");
    t.took();
    t.writer.dropped();
    t.writer.settle();
    assert_eq!(
        t.done(),
        Some(JobResult::Dropped {
            sent: 1,
            posted: false
        })
    );
    assert_eq!(t.writer.state(0).game, Game::Unknown);
}

#[test]
fn checks_your_description_once() {
    let mut t = Table::new();
    assert_eq!(
        t.run(WriterCommand::Start(job(
            1,
            Kind::Description,
            Action::Check,
            &[]
        ))),
        vec!["dcheck"]
    );
    t.answer(&[
        "Your description has been sent for approval. ALL dcheck submissions will",
        "receive an automated note when a decision has been reached.",
    ]);
    match t.done() {
        Some(JobResult::Checked { lines }) => assert_eq!(lines.len(), 2),
        other => panic!("{other:?}"),
    }
}

/// A find on `kind` for TEXT's note, which asks the board's list.
fn find(kind: Kind) -> (Table, WriteJob) {
    let mut spec = note(1, Action::Find);
    spec.kind = kind;
    spec.lines.clear();
    (Table::new(), spec)
}

#[test]
fn finds_your_note_on_the_boards_list_and_sends_nothing_else() {
    let (mut t, spec) = find(Kind::Journal);
    assert_eq!(t.run(WriterCommand::Start(spec)), vec!["journal list"]);
    t.answer(&[
        " [  2 ] Maren: The Great Milieu",
        "[  3N] Orla: The Great Milieu",
    ]);
    assert_eq!(t.done(), Some(JobResult::Found { number: 3 }));
    assert_eq!(t.sent, vec!["journal list"]);
}

#[test]
fn finds_your_note_after_a_cabal_and_before_its_language() {
    let (mut t, spec) = find(Kind::Note);
    assert_eq!(t.run(WriterCommand::Start(spec)), vec!["note list"]);
    t.answer(&[" [  5N] [Knight] Orla: The Great Milieu (dwarvish)"]);
    assert_eq!(t.done(), Some(JobResult::Found { number: 5 }));
}

#[test]
fn turns_the_pager_for_a_long_list() {
    let (mut t, spec) = find(Kind::Note);
    t.run(WriterCommand::Start(spec));
    assert_eq!(
        t.game(
            &[" [  0 ] Maren: About the gate"],
            "\r[Hit Return to continue]\r"
        ),
        vec![""]
    );
    assert_eq!(t.done(), None);
    t.answer(&["[Hit Return to continue] [ 41 ] Orla: The Great Milieu"]);
    assert_eq!(t.done(), Some(JobResult::Found { number: 41 }));
    assert_eq!(t.sent, vec!["note list", ""]);
}

#[test]
fn says_when_the_list_holds_no_such_note() {
    let (mut t, spec) = find(Kind::Note);
    t.run(WriterCommand::Start(spec));
    t.answer(&[" [  0 ] Orla: About the gate"]);
    assert_eq!(t.done(), Some(JobResult::NotFound));
    let (mut t, spec) = find(Kind::Note);
    t.run(WriterCommand::Start(spec));
    t.answer(&["There are no notes for you."]);
    assert_eq!(t.done(), Some(JobResult::NotFound));
}

#[test]
fn cannot_tell_on_a_board_only_immortals_read() {
    let (mut t, spec) = find(Kind::Idea);
    assert_eq!(t.run(WriterCommand::Start(spec)), vec!["idea list"]);
    t.answer(&["Only immortals may read ideas."]);
    assert_eq!(t.done(), Some(JobResult::CantTell));
}
