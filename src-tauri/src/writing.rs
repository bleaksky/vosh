//! Your writing, kept in `writing.toml` in the app data folder. Each
//! character on each world keeps its drafts, newest
//! first, and its last 20 posts under Sent, with the game's copy of each
//! text the game saves in place and the race and level last seen, so a
//! werebeast keeps its Beast switch after a login that sends no
//! Char.Status. The card's own switches, Check
//! spelling and the guide, sit at the top of the file.
//!
//! A file of its own means a keystroke never rewrites a profile. The page
//! saves one character at a time, and each save reads the file, swaps
//! that character in and writes the whole file back while it holds the
//! lock, so a save never drops another character. A file Vosh cannot
//! read stays as it is, and the card works from memory until you quit.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The shape this build writes.
const FILE_VERSION: i64 = 1;

/// How many posts a character keeps under Sent.
pub(crate) const SENT_KEPT: usize = 20;

/// One save at a time, so two never interleave their reads and writes.
static FILE_LOCK: Mutex<()> = Mutex::new(());

fn yes() -> bool {
    true
}

fn version() -> i64 {
    FILE_VERSION
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WritingFile {
    #[serde(default = "version")]
    pub(crate) version: i64,
    /// The card checks spelling as you type.
    #[serde(default = "yes")]
    pub(crate) spelling: bool,
    /// The guide shows beside the text. It opens the first time and
    /// remembers when you close it.
    #[serde(default = "yes")]
    pub(crate) guide: bool,
    /// Each character, by world and name, see [`character_key`].
    #[serde(default)]
    pub(crate) characters: BTreeMap<String, Character>,
}

impl Default for WritingFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            spelling: true,
            guide: true,
            characters: BTreeMap::new(),
        }
    }
}

/// One character's writing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Character {
    /// The world, by the host and port the session dials.
    pub(crate) host: String,
    pub(crate) port: u16,
    /// The name as the game spells it.
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) race: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) level: Option<i64>,
    /// The beast `beastdesc edit` named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) beast: Option<String>,
    #[serde(default)]
    pub(crate) drafts: Vec<Draft>,
    #[serde(default)]
    pub(crate) sent: Vec<Draft>,
}

/// A draft, or a post under Sent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Draft {
    pub(crate) id: String,
    /// The kind of text, as the writer names it.
    pub(crate) kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    /// An application for a custom race, which keeps to 70 a line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) custom_race: bool,
    /// The room a bug or typo report began in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) room: Option<String>,
    /// The text, one line each.
    #[serde(default)]
    pub(crate) text: Vec<String>,
    /// The game's copy of a text it saves in place, when Vosh knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) game: Option<Vec<String>>,
    /// When it last changed, or when it posted, in milliseconds since
    /// the Unix epoch.
    #[serde(default)]
    pub(crate) at: i64,
}

/// The key of a character on a world: `{host}:{port} {name}`, the name
/// in lower case, as the affect fulls key theirs.
fn character_key(host: &str, port: u16, name: &str) -> String {
    crate::affects::full::character_key(host, port, name)
}

/// The file at `path`, or the defaults when there is none. An error when
/// it is there and does not read.
pub(crate) fn read(path: &Path) -> Result<WritingFile, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(WritingFile::default()),
        Err(e) => return Err(e.to_string()),
    };
    toml::from_str(&text).map_err(|e| e.to_string())
}

/// Swap `character` in under its key, or take it out when it holds no
/// draft, no post and nothing the card keeps for it, and write the file
/// back whole. Sent keeps its newest [`SENT_KEPT`].
pub(crate) fn save_character(path: &Path, mut character: Character) -> Result<(), String> {
    let _guard = FILE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut file = read(path)?;
    let key = character_key(&character.host, character.port, &character.name);
    character
        .sent
        .sort_by_key(|draft| std::cmp::Reverse(draft.at));
    character.sent.truncate(SENT_KEPT);
    let empty = character.drafts.is_empty()
        && character.sent.is_empty()
        && character.race.is_none()
        && character.level.is_none()
        && character.beast.is_none();
    if empty {
        file.characters.remove(&key);
    } else {
        file.characters.insert(key, character);
    }
    write(path, &file)
}

/// Keep the card's two switches.
pub(crate) fn save_switches(path: &Path, spelling: bool, guide: bool) -> Result<(), String> {
    let _guard = FILE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut file = read(path)?;
    file.spelling = spelling;
    file.guide = guide;
    write(path, &file)
}

fn write(path: &Path, file: &WritingFile) -> Result<(), String> {
    let text = toml::to_string(file).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::disk::atomic::swap_in(path, &text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(id: &str, at: i64) -> Draft {
        Draft {
            id: id.into(),
            kind: "note".into(),
            subject: "The Great Milieu".into(),
            to: "all".into(),
            text: vec!["A line.".into(), String::new()],
            at,
            ..Draft::default()
        }
    }

    fn character(name: &str) -> Character {
        Character {
            host: "play.theforsakenlands.com".into(),
            port: 1848,
            name: name.into(),
            ..Character::default()
        }
    }

    #[test]
    fn keeps_each_character_apart_and_the_newest_twenty_posts() {
        let dir = tempfile::tempdir().expect("a folder");
        let path = dir.path().join("writing.toml");
        assert_eq!(read(&path), Ok(WritingFile::default()));
        let orla = Character {
            race: Some("elf".into()),
            level: Some(30),
            drafts: vec![draft("a", 1)],
            sent: (0..25).map(|n| draft(&n.to_string(), n)).collect(),
            ..character("Orla")
        };
        save_character(&path, orla.clone()).expect("a save");
        save_character(
            &path,
            Character {
                drafts: vec![draft("b", 2)],
                ..character("Maren")
            },
        )
        .expect("a save");
        let file = read(&path).expect("the file");
        assert_eq!(file.characters.len(), 2);
        let kept = &file.characters["play.theforsakenlands.com:1848 orla"];
        assert_eq!(kept.sent.len(), SENT_KEPT);
        assert_eq!(kept.sent[0].id, "24");
        assert_eq!(kept.drafts, orla.drafts);
        // A character with nothing left goes.
        save_character(&path, character("Orla")).expect("a save");
        assert_eq!(read(&path).expect("the file").characters.len(), 1);
        save_switches(&path, false, true).expect("a save");
        let file = read(&path).expect("the file");
        assert!(!file.spelling);
        assert!(file.guide);
    }

    #[test]
    fn leaves_a_file_it_cannot_read_alone() {
        let dir = tempfile::tempdir().expect("a folder");
        let path = dir.path().join("writing.toml");
        std::fs::write(&path, "characters = 5").expect("a write");
        assert!(read(&path).is_err());
        assert!(save_character(&path, character("Orla")).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file"),
            "characters = 5"
        );
    }
}
