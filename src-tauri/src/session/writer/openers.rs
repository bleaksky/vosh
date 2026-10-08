//! Which text a line you typed opens in the game's editor, read the way
//! the game reads it. `interpret` runs the first
//! command in its table that the word starts (`interp.c:1222`), so a
//! short word reaches an editor only when no command before it starts
//! the same way. The banner never names the text (`olc.c:3385`), so the
//! session takes this answer and the line the game prints first to know
//! what the editor holds.

use super::kinds::Kind;

/// Each command that opens an editor, with the fewest letters that reach
/// it. The commands before it in `cmd_table` that share more letters
/// take any shorter word: `north`, `nofollow` and `nosummon` take `no`
/// (`interp.c:73`, `223`, `224`), `judge` and `jail` take `j`, a command
/// such as `affects` takes `a`, `inventory` takes `i`, `buy` takes `bu`
/// (`interp.c:88`), `tell` and `time` take `t`, `hit`, `hide` and
/// `high` take `hi` (`interp.c:97`), `down` takes `d`, `beastcall` and
/// `beastial` take `beast` (`interp.c:685`), `newbiechat` takes `new`
/// (`interp.c:114`), `channels` takes `chan` (`interp.c:89`), and
/// `permban`, `perkset` and `peace` take `pe` for an immortal. No
/// command that waits for a level takes more letters than these.
const COMMANDS: &[(&str, usize)] = &[
    ("note", 3),
    ("journal", 2),
    ("application", 2),
    ("idea", 2),
    ("bug", 3),
    ("typo", 2),
    ("history", 3),
    ("description", 2),
    ("beastdesc", 6),
    ("news", 4),
    ("changes", 5),
    ("penalty", 3),
];

/// The command `word` runs, among those that open an editor, or None.
fn command(word: &str) -> Option<&'static str> {
    let word = word.to_ascii_lowercase();
    COMMANDS
        .iter()
        .find(|(name, least)| word.len() >= *least && name.starts_with(word.as_str()))
        .map(|(name, _)| *name)
}

/// The text `line` opens the game's editor on, or None when it opens
/// none or Vosh cannot tell. `description` and `beastdesc` take `edit`
/// alone after them (`act_info.c:7550`), a board takes `edit` as its
/// first word (`recycle.c:4335`), and `history` takes `edit`,
/// `personality` or `purpose` (`act_comm.c:5109` to `5123`), each word
/// in full.
pub(crate) fn opens(line: &str) -> Option<Kind> {
    let line = line.trim();
    let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
    let rest = rest.trim();
    let first = rest.split_whitespace().next().unwrap_or("");
    let is = |want: &str| first.eq_ignore_ascii_case(want);
    let board = |kind: Kind| is("edit").then_some(kind);
    match command(word)? {
        "description" => rest
            .eq_ignore_ascii_case("edit")
            .then_some(Kind::Description),
        "beastdesc" => rest.eq_ignore_ascii_case("edit").then_some(Kind::Beast),
        "history" if is("edit") => Some(Kind::History),
        "history" if is("personality") => Some(Kind::Personality),
        "history" if is("purpose") => Some(Kind::Purpose),
        "note" => board(Kind::Note),
        "journal" => board(Kind::Journal),
        "application" => board(Kind::Application),
        "idea" => board(Kind::Idea),
        "bug" => board(Kind::Bug),
        "typo" => board(Kind::Typo),
        "news" => board(Kind::News),
        "changes" => board(Kind::Changes),
        "penalty" => board(Kind::Penalty),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_opener_in_full_and_short() {
        assert_eq!(opens("description edit"), Some(Kind::Description));
        assert_eq!(opens("desc edit"), Some(Kind::Description));
        assert_eq!(opens("de edit"), Some(Kind::Description));
        assert_eq!(opens("beastdesc edit"), Some(Kind::Beast));
        assert_eq!(opens("beastd edit"), Some(Kind::Beast));
        assert_eq!(opens("note edit"), Some(Kind::Note));
        assert_eq!(opens("not edit"), Some(Kind::Note));
        assert_eq!(opens("jo edit"), Some(Kind::Journal));
        assert_eq!(opens("ap edit"), Some(Kind::Application));
        assert_eq!(opens("id edit"), Some(Kind::Idea));
        assert_eq!(opens("bug edit"), Some(Kind::Bug));
        assert_eq!(opens("ty edit"), Some(Kind::Typo));
        assert_eq!(opens("his edit"), Some(Kind::History));
        assert_eq!(opens("history personality"), Some(Kind::Personality));
        assert_eq!(opens("history purpose"), Some(Kind::Purpose));
        assert_eq!(opens("NOTE EDIT"), Some(Kind::Note));
    }

    #[test]
    fn leaves_words_another_command_takes() {
        // north, buy, hit, beastcall and newbiechat come first.
        assert_eq!(opens("no edit"), None);
        assert_eq!(opens("bu edit"), None);
        assert_eq!(opens("hi edit"), None);
        assert_eq!(opens("beast edit"), None);
        assert_eq!(opens("new edit"), None);
        assert_eq!(opens("d edit"), None);
    }

    #[test]
    fn wants_the_word_after_in_full() {
        // The game answers note ed with You can't do that.
        assert_eq!(opens("note ed"), None);
        assert_eq!(opens("description edit now"), None);
        assert_eq!(opens("description"), None);
        assert_eq!(opens("note to Maren"), None);
        assert_eq!(opens("note + a line"), None);
        assert_eq!(opens("history show"), None);
        assert_eq!(opens("history pers"), None);
        // A board reads only its first word.
        assert_eq!(opens("note edit please"), Some(Kind::Note));
    }
}
