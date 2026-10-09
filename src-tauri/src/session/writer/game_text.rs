//! What the game prints around its line editor, and what it does to a
//! line it takes. Each item names the server code it follows, so the
//! card reads the game's own words and nothing it guesses.

use vosh_prompt::aabahran::colors::rebuild;

use super::kinds::Kind;

/// The banner `string_append` prints as the editor opens
/// (`olc.c:3385` to `3388`).
pub(crate) const BANNER: [&str; 4] = [
    "-=======- Entering APPEND Mode -========-",
    "    Type .h on a new line for help",
    " Terminate with a ~ or @ on a blank line.",
    "-=======================================-",
];

/// What the editor prints after `.c` (`olc.c:3626`).
pub(crate) const CLEARED: &str = "String cleared.";

/// What the editor prints after `.i` puts a line in (`olc.c:3560`).
pub(crate) const INSERTED: &str = "Line inserted.";

/// What the editor prints after `.f` wraps the text (`olc.c:3724`).
pub(crate) const FORMATTED: &str = "String formatted.";

/// What the editor prints when a line would take the text past what it
/// holds, before it closes (`olc.c:3836`).
pub(crate) const TOO_LONG: &str = "String too long, last line skipped.";

/// What the editor prints for a dot command it does not know
/// (`olc.c:3743`).
pub(crate) const BAD_DOT: &str = "SEdit:  Invalid dot command.";

/// What `string_append` prints before a long text, which is no part of
/// it (`olc.c:3402`).
pub(crate) const BUFFER_IS: &str = "Buffer is ";

/// The pager's prompt (`comm.c:1581`).
pub(crate) const PAGER: &str = "[Hit Return to continue]";

/// What a board answers a field, `clear` and `post` with when it takes
/// them (`recycle.c:4416`, `4505`, `4513`, `4805`).
pub(crate) const OK: &str = "Ok.";

/// A board with no note in progress (`recycle.c:4523`).
pub(crate) const NO_NOTE: &str = "You have no note in progress.";

/// A board asked about a note you started on another one
/// (`recycle.c:4528`).
pub(crate) const OTHER_BOARD: &str = "You aren't working on that kind of note.";

/// A cabal application the game turned into a vote (`vote.c:1593`).
pub(crate) const ACCEPTED: &str = "Your application has been accepted.";

/// A bug or typo report the forum missed (`recycle.c:4738`).
pub(crate) const NO_FORUM: &str = "Your report was saved in game but could not be posted to the forum. Ask for help on our Discord if this keeps happening.";

/// The note language answers, for a tongue and for Common
/// (`recycle.c:4429`, `4442`).
pub(crate) const LANGUAGE_SET: [&str; 2] = [
    "Your note will be written in ",
    "Your note will be readable in Common.",
];

/// What `list` tells a mortal on a board only immortals read
/// (`recycle.c:3963`, `3969`). The typo board lists your own reports to
/// you (`recycle.c:2516`).
pub(crate) const STAFF_ONLY: [&str; 2] = [
    "Only immortals may read ideas.",
    "Only immortals may read bug reports.",
];

/// What the game tells you when the immortals approve your description,
/// after the name of your god, The One God when you follow none
/// (`act_wiz.c:14806`, `14837`, `comm.c:7453`). It ends there or goes on
/// to the blessing.
pub(crate) const JUDGED: [&str; 2] = [
    " has judged your look worthy.",
    " has judged your look worthy and grants you a blessing.",
];

/// What the game tells you when the immortals turn your description down
/// with a penalty (`act_wiz.c:14823`).
pub(crate) const ANGERED: &str = "Your poor description has angrered ";

/// What the game tells you when the immortals award your history
/// (`rppoints.c:908`, `act_wiz.c:15021`).
pub(crate) const BLESSED: &str = "As the gods view your past, they grant you a small blessing.";

/// The check `plain` says the game decided, for a line that does.
pub(crate) fn decided(plain: &str) -> Option<Kind> {
    let plain = uncoded(plain);
    if plain == BLESSED {
        return Some(Kind::History);
    }
    let judged = JUDGED.iter().any(|end| {
        plain
            .strip_suffix(end)
            .is_some_and(|god| !god.is_empty() && !god.contains('\''))
    });
    (judged || plain.starts_with(ANGERED)).then_some(Kind::Description)
}

/// What a text the game holds none of prints (`act_info.c:7623`).
pub(crate) const NONE: &str = "(None).";

/// A line the game sent, its carriage returns gone, with its bytes, which
/// keep the colors a read back maps to codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameLine {
    pub(crate) plain: String,
    pub(crate) raw: Vec<u8>,
}

impl GameLine {
    pub(crate) fn new(plain: &str, raw: &[u8]) -> Self {
        Self {
            plain: plain.replace('\r', ""),
            raw: raw.to_vec(),
        }
    }

    /// The line with its colors as backtick codes, the way you type them.
    pub(crate) fn coded(&self) -> String {
        rebuild(&String::from_utf8_lossy(&self.raw).replace('\r', ""))
    }
}

/// How many times the game's editor prompt `> ` makes up `text` alone, a
/// read that brought nothing else. The game writes one after every line
/// it takes (`comm.c:1583`), and one after anything else it sends while
/// the editor is open, so only a `>` with nothing beside it counts.
pub(crate) fn lone_prompts(text: &str) -> usize {
    let text = text.replace('\r', "");
    let mut rest = text.as_str();
    let mut n = 0;
    while let Some(after) = rest.strip_prefix("> ") {
        rest = after;
        n += 1;
    }
    if rest.is_empty() {
        n
    } else {
        0
    }
}

/// The editor puts a line you send into the text: anything but a dot
/// command or the `@` that ends it (`olc.c:3611`, `3746`).
pub(crate) fn takes(line: &str) -> bool {
    !line.starts_with(['.', '@'])
}

/// The editor waits for a line: the partial a read ended on is its `> `,
/// once or more. `.s` on an empty text prints its first number and no
/// line end (`olc.c:3647`), so that number may come before it.
pub(crate) fn editor_waits(partial: &str) -> bool {
    let partial = partial.replace('\r', "");
    let mut rest = partial.as_str();
    let mut prompts = 0;
    while let Some(before) = rest.strip_suffix("> ") {
        rest = before;
        prompts += 1;
    }
    prompts > 0
        && (rest.is_empty() || listing_number(rest).is_some_and(|(_, text)| text.is_empty()))
}

/// The line `.d` took out, from what the editor prints after it
/// (`olc.c:3717`, `Line %d deleted.`). It prints the number even for a
/// line the text does not hold.
pub(crate) fn deleted(line: &str) -> Option<usize> {
    line.strip_prefix("Line ")?
        .strip_suffix(" deleted.")?
        .parse()
        .ok()
}

/// How many lines a `.s` in this read listed, or None for a read with
/// no listing. On an empty text `.s` prints its first number alone
/// before the `> ` (`olc.c:3647`).
pub(crate) fn shown_lines(lines: &[GameLine], partial: &str) -> Option<usize> {
    let held = listing(lines).len();
    if held > 0 {
        return Some(held);
    }
    let partial = partial.replace('\r', "");
    let number = partial.trim_end_matches("> ");
    (number != partial && listing_number(number) == Some((1, ""))).then_some(0)
}

/// The game's pager waits for Return.
pub(crate) fn pager_waits(partial: &str) -> bool {
    partial.contains(PAGER)
}

/// The number `.s` puts before a line and the line after it. The number
/// is two wide (`olc.c:3647`, `%2d`), then one space, then the line as
/// stored.
fn listing_number(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start_matches(' ');
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || line.len() - trimmed.len() > 1 {
        return None;
    }
    let n = trimmed[..digits].parse().ok()?;
    let rest = &trimmed[digits..];
    let text = rest.strip_prefix(' ').unwrap_or(rest);
    Some((n, text))
}

/// The text `.s` listed, from its lines. A `> ` the game wrote before the
/// listing, for a line it took in the same pulse, goes.
pub(crate) fn listing(lines: &[GameLine]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let mut plain = line.plain.as_str();
        while let Some(after) = plain.strip_prefix("> ") {
            plain = after;
        }
        if let Some((n, text)) = listing_number(plain) {
            if n == out.len() + 1 {
                out.push(text.trim_end_matches(' ').to_string());
            }
        }
    }
    out
}

/// The text the editor listed as it opened, in place of the banner's
/// lines and its `Buffer is` line, colors as codes. An empty text lists
/// one empty line (`olc.c:3407`).
pub(crate) fn opened_listing(lines: &[GameLine]) -> Vec<String> {
    let banner = lines.iter().position(|l| l.plain == BANNER[0]);
    let start = banner.map_or(0, |at| at + BANNER.len());
    let mut out: Vec<String> = lines[start.min(lines.len())..]
        .iter()
        .filter(|l| !l.plain.starts_with(BUFFER_IS))
        .map(|l| l.coded().trim_end_matches(' ').to_string())
        .collect();
    if out.len() == 1 && out[0].is_empty() {
        out.clear();
    }
    out
}

/// What the game stores for a line you send: only printable ASCII stays
/// (`comm.c:1504`), and below trust 55 a backtick and the character after
/// it go unless they open the line (`comm.c:1499`). Then `~` turns to `-`
/// and `"` to `'` (`olc.c:3830`), and the editor keeps no space at the
/// end. An empty line arrives as one space (`comm.c:1506`).
pub(crate) fn stored(line: &str, immortal: bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' && !out.is_empty() && !immortal && chars.peek().is_some() {
            chars.next();
            continue;
        }
        if c.is_ascii() && !c.is_ascii_control() {
            out.push(match c {
                '~' => '-',
                '"' => '\'',
                other => other,
            });
        }
    }
    out.trim_end_matches(' ').to_string()
}

/// Whether a line the game holds is the line you meant, the trailing
/// spaces of both aside.
pub(crate) fn same(held: &str, meant: &str) -> bool {
    held.trim_end_matches(' ') == meant.trim_end_matches(' ')
}

/// The line as the card sends it: its own text, or one space for an
/// empty line, which the game keeps as a paragraph break.
pub(crate) fn wire(line: &str) -> String {
    let line = line.trim_end_matches(' ');
    if line.is_empty() {
        " ".to_string()
    } else {
        line.to_string()
    }
}

/// A line the editor would not take as text: a dot runs a dot command,
/// `@` ends the editor and `!` repeats your last line (`olc.c:3613`,
/// `3746`, `comm.c:1560`).
pub(crate) fn needs_mend(line: &str) -> bool {
    matches!(line.chars().next(), Some('.' | '@' | '!'))
}

/// `.rl` for line `n`, its text inside the first mark `first_arg` takes
/// that the text does not hold (`olc.c:4300`), or None when it holds
/// them all.
pub(crate) fn replace_line(n: usize, text: &str) -> Option<String> {
    let text = if text.is_empty() { " " } else { text };
    [('\'', '\''), ('%', '%'), ('(', ')'), ('"', '"')]
        .into_iter()
        .find(|(_, close)| !text.contains(*close))
        .map(|(open, close)| format!(".rl {n} {open}{text}{close}"))
}

/// The text a reply to `description` or `beastdesc` holds, after the
/// header `header` (`act_info.c:7623`, `7723`), colors as codes. The
/// empty row a session without compact gets before its prompt goes
/// (`comm.c:1621`), and `(None).` is no text at all. None when the
/// header never came.
pub(crate) fn after_header(lines: &[GameLine], headers: &[&str]) -> Option<Vec<String>> {
    let at = lines
        .iter()
        .position(|l| headers.contains(&l.plain.as_str()))?;
    let mut text: Vec<String> = lines[at + 1..]
        .iter()
        .map(|l| l.coded().trim_end_matches(' ').to_string())
        .collect();
    if text.last().is_some_and(String::is_empty) && lines.last().is_some_and(|l| l.plain.is_empty())
    {
        text.pop();
    }
    if text.len() == 1 && text[0] == NONE {
        text.clear();
    }
    Some(text)
}

/// A note as `show` prints it (`recycle.c:4556`): who and the subject,
/// To, a Language line when it has one, then the text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ShownNote {
    pub(crate) subject: String,
    pub(crate) to: String,
    pub(crate) language: Option<String>,
    pub(crate) lines: Vec<String>,
}

/// The note in a reply to `show`, or None when the reply is not one.
/// The first line puts a rank or a cabal before the sender's name
/// (`recycle.c:4530` to `4553`), so the subject is what follows `name`
/// and its colon, or the first colon when no name is given.
pub(crate) fn shown_note(lines: &[GameLine], name: Option<&str>) -> Option<ShownNote> {
    let to_at = lines.iter().position(|l| l.plain.starts_with("To: "))?;
    let head = lines.get(to_at.checked_sub(1)?)?;
    let subject = match name {
        Some(name) => head
            .plain
            .find(&format!("{name}: "))
            .map(|at| &head.plain[at + name.len() + 2..])?,
        None => head.plain.split_once(": ").map(|(_, s)| s)?,
    };
    let to = lines[to_at].plain["To: ".len()..].to_string();
    let mut at = to_at + 1;
    let language = lines
        .get(at)
        .and_then(|l| l.plain.strip_prefix("Language: "))
        .map(str::to_string);
    if language.is_some() {
        at += 1;
    }
    let mut text: Vec<String> = lines[at..]
        .iter()
        .map(|l| l.coded().trim_end_matches(' ').to_string())
        .collect();
    if text.last().is_some_and(String::is_empty) && lines.last().is_some_and(|l| l.plain.is_empty())
    {
        text.pop();
    }
    Some(ShownNote {
        subject: subject.trim_end().to_string(),
        to: to.trim_end().to_string(),
        language,
        lines: text,
    })
}

/// A row of a board's `list` (`recycle.c:4067`), its colors gone: a mark
/// for a note that waits too long, `[`, an `a` for an awarded note, the
/// number three wide, an `N` for one you have not read, `]`, then a rank
/// or a cabal, the sender, a colon and the subject, with the note's
/// language in brackets after it when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListRow<'a> {
    pub(crate) number: usize,
    pub(crate) sender: &'a str,
    /// The subject, the language after it included.
    pub(crate) subject: &'a str,
}

/// The row in `plain`, or None when it is no row of a list. The pager's
/// prompt can come before the first row of a page.
pub(crate) fn list_row(plain: &str) -> Option<ListRow<'_>> {
    let row = plain.strip_prefix(PAGER).unwrap_or(plain);
    let row = row.strip_prefix(['!', ' ']).unwrap_or(row);
    let (inside, rest) = row.strip_prefix('[')?.split_once("] ")?;
    let number = inside.get(1..inside.len().checked_sub(1)?)?;
    let number = number.trim_start().parse().ok()?;
    let (head, subject) = rest.split_once(": ")?;
    let sender = head.rsplit(' ').next()?;
    Some(ListRow {
        number,
        sender,
        subject: subject.trim_end(),
    })
}

/// The numbers of the notes `name` sent with `subject` in a reply to
/// `list`, in order, the note's language after the subject aside.
pub(crate) fn listed_notes(lines: &[GameLine], name: &str, subject: &str) -> Vec<usize> {
    let titled = |listed: &str| {
        listed == subject
            || listed
                .strip_prefix(subject)
                .and_then(|tag| tag.strip_prefix(" ("))
                .and_then(|tag| tag.strip_suffix(')'))
                .is_some_and(|tag| !tag.is_empty() && !tag.contains(['(', ')']))
    };
    lines
        .iter()
        .filter_map(|l| list_row(&l.plain))
        .filter(|row| row.sender == name && titled(row.subject))
        .map(|row| row.number)
        .collect()
}

/// `text` as the game shows it, without its backtick codes, each of
/// which takes the character after it.
pub(crate) fn uncoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '`' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(plain: &str) -> GameLine {
        GameLine::new(plain, plain.as_bytes())
    }

    #[test]
    fn the_editor_takes_a_line_but_no_dot_command_or_at() {
        assert!(takes("Orla watches the gate."));
        assert!(takes(""));
        assert!(!takes(".s"));
        assert!(!takes("./ look"));
        assert!(!takes("@"));
    }

    #[test]
    fn counts_a_prompt_only_when_it_comes_alone() {
        assert_eq!(lone_prompts("> "), 1);
        assert_eq!(lone_prompts("> > > "), 3);
        assert_eq!(lone_prompts("\rTolliver says 'hi'\n\r> "), 0);
        assert_eq!(lone_prompts(""), 0);
    }

    #[test]
    fn knows_when_the_editor_waits() {
        assert!(editor_waits("> "));
        assert!(editor_waits("\r> > "));
        assert!(editor_waits(" 1 > "));
        assert!(!editor_waits("<1020hp 800m 930mv> "));
        assert!(!editor_waits("\r[Hit Return to continue]\r"));
        assert!(pager_waits("\r[Hit Return to continue]\r"));
    }

    #[test]
    fn reads_what_the_editor_says_it_did() {
        assert_eq!(deleted("Line 2 deleted."), Some(2));
        assert_eq!(deleted("Line two deleted."), None);
        assert_eq!(deleted("Tolliver deleted."), None);
        let shown = [line(" 1 one"), line(" 2 two")];
        assert_eq!(shown_lines(&shown, "> "), Some(2));
        assert_eq!(shown_lines(&[], " 1 > "), Some(0));
        assert_eq!(shown_lines(&[], "> "), None);
        assert_eq!(shown_lines(&[line("Tolliver says 'hi'")], "> "), None);
    }

    #[test]
    fn reads_a_listing_by_its_numbers() {
        let lines = [
            line(" 1 This tall elf stands"),
            line(" 2 "),
            line(" 3    with an indent  "),
            line("10 a tenth line"),
        ];
        assert_eq!(
            listing(&lines),
            vec!["This tall elf stands", "", "   with an indent"]
        );
        let merged = [line("> > 1 one"), line(" 2 two")];
        assert_eq!(listing(&merged), vec!["one", "two"]);
    }

    #[test]
    fn stores_a_line_as_the_game_does() {
        assert_eq!(
            stored("He said \"no\" ~ twice", false),
            "He said 'no' - twice"
        );
        assert_eq!(stored("`!Red then `` plain", false), "`!Red then  plain");
        assert_eq!(stored("`!Red then `` plain", true), "`!Red then `` plain");
        assert_eq!(stored("caf\u{e9} \t end  ", false), "caf  end");
        assert_eq!(wire(""), " ");
        assert_eq!(wire("text  "), "text");
    }

    #[test]
    fn replaces_a_line_inside_a_mark_it_does_not_hold() {
        assert_eq!(replace_line(3, "plain"), Some(".rl 3 'plain'".into()));
        assert_eq!(replace_line(3, "it's"), Some(".rl 3 %it's%".into()));
        assert_eq!(replace_line(3, "it's 5%"), Some(".rl 3 (it's 5%)".into()));
        assert_eq!(replace_line(3, ""), Some(".rl 3 ' '".into()));
        assert_eq!(replace_line(3, "'%)\""), None);
    }

    #[test]
    fn reads_the_text_after_the_header() {
        let reply = [
            line("Your description is:"),
            line("A tall elf."),
            line(" "),
            line("Second paragraph."),
            line(""),
        ];
        assert_eq!(
            after_header(&reply, &["Your description is:"]),
            Some(vec![
                "A tall elf.".into(),
                String::new(),
                "Second paragraph.".into()
            ])
        );
        let none = [line("Your description is:"), line("(None)."), line("")];
        assert_eq!(after_header(&none, &["Your description is:"]), Some(vec![]));
        assert_eq!(
            after_header(&[line("Huh?")], &["Your description is:"]),
            None
        );
    }

    #[test]
    fn reads_a_shown_note() {
        let reply = [
            line("Tolliver: The Great Milieu"),
            line("To: all"),
            line("First line"),
            line(""),
        ];
        let note = shown_note(&reply, Some("Tolliver")).expect("a note");
        assert_eq!(note.subject, "The Great Milieu");
        assert_eq!(note.to, "all");
        assert_eq!(note.language, None);
        assert_eq!(note.lines, vec!["First line"]);
        let ranked = [
            line("IMM Tolliver: Plans: a list"),
            line("To: immortal"),
            line("Language: Elvish"),
        ];
        let note = shown_note(&ranked, Some("Tolliver")).expect("a note");
        assert_eq!(note.subject, "Plans: a list");
        assert_eq!(note.language.as_deref(), Some("Elvish"));
        assert_eq!(note.lines, Vec::<String>::new());
    }

    #[test]
    fn reads_the_rows_of_a_list() {
        assert_eq!(
            list_row("[  3N] Orla: About the gate"),
            Some(ListRow {
                number: 3,
                sender: "Orla",
                subject: "About the gate",
            })
        );
        let ranked = list_row("![a 12 ] IMM Tolliver: Plans: a list (drenish)").expect("a row");
        assert_eq!(ranked.number, 12);
        assert_eq!(ranked.sender, "Tolliver");
        assert_eq!(ranked.subject, "Plans: a list (drenish)");
        assert_eq!(
            list_row("[Hit Return to continue] [ 1000N] Maren: Late").map(|r| r.number),
            Some(1000)
        );
        assert_eq!(list_row("There are no notes for you."), None);
        let rows = [
            line(" [  0 ] Maren: About the gate"),
            line(" [  1N] Orla: About the gate (foreign)"),
            line(" [  2 ] Orla: About the gate now"),
        ];
        assert_eq!(listed_notes(&rows, "Orla", "About the gate"), vec![1]);
        assert_eq!(listed_notes(&rows, "Orla", "About"), Vec::<usize>::new());
        assert_eq!(uncoded("`!Red`` then plain"), "Red then plain");
    }

    #[test]
    fn knows_when_the_game_decides_a_check() {
        assert_eq!(
            decided("Orla has judged your look worthy."),
            Some(Kind::Description)
        );
        assert_eq!(
            decided("The One God has judged your look worthy."),
            Some(Kind::Description)
        );
        assert_eq!(
            decided("Orla has judged your look worthy and grants you a blessing."),
            Some(Kind::Description)
        );
        assert_eq!(
            decided("Your poor description has angrered Orla!"),
            Some(Kind::Description)
        );
        assert_eq!(decided(BLESSED), Some(Kind::History));
        assert_eq!(
            decided("`^As the gods view your past, they grant you a small blessing.``"),
            Some(Kind::History)
        );
        assert_eq!(decided("Maren says 'your look worthy'"), None);
        assert_eq!(
            decided("Maren says 'Orla has judged your look worthy.'"),
            None
        );
        assert_eq!(decided(" has judged your look worthy."), None);
        assert_eq!(decided("You earn 3 rp points!"), None);
    }
}
