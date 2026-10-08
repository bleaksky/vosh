//! Who the prompt is for. The engine follows it from Char.Status and
//! Char.State, and the PROMPT compiler asks it what your setting prints.

/// Who the prompt is for, which decides what `%u` and `%s` print and
/// what the game keeps of a setting you type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Who {
    /// An immortal, for whom `%u` prints `pacified` or `not pacified`.
    pub immortal: bool,
    /// You control a mobile, which leaves `%s` repeating the text of the
    /// code before it.
    pub mobile: bool,
    /// The game keeps the backticks you type. For anyone with trust under
    /// 55 it drops each one and the character after it as it reads the
    /// line (`read_from_buffer`, `comm.c:1496-1497`).
    pub keeps_backticks: bool,
}

/// The level above which the game counts you as an immortal
/// (`LEVEL_IMMORTAL`, `merc.h`), for `%u`.
pub(crate) const LEVEL_IMMORTAL: i64 = 51;

/// The trust from which the game keeps the backticks you type. Vosh
/// reads it from the level in Char.Status, which is your trust unless an
/// immortal set another.
pub(crate) const TRUST_BACKTICKS: i64 = 55;

impl Who {
    /// Who the prompt is for, from the packets the game sent: the level
    /// in Char.Status, and Char.State, whose language is empty while you
    /// control a mobile. Without a packet Vosh takes you
    /// for a mortal in your own body.
    pub(crate) fn from_packets(level: Option<i64>, language: Option<&str>) -> Self {
        Self {
            immortal: level.is_some_and(|l| l > LEVEL_IMMORTAL),
            mobile: language.is_some_and(str::is_empty),
            keeps_backticks: level.is_some_and(|l| l >= TRUST_BACKTICKS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_follows_the_level_and_the_language() {
        assert_eq!(Who::from_packets(None, None), Who::default());
        assert!(!Who::from_packets(Some(51), None).immortal);
        assert!(Who::from_packets(Some(52), None).immortal);
        assert!(!Who::from_packets(None, Some("common")).mobile);
        assert!(Who::from_packets(Some(60), Some("")).mobile);
        // The game keeps the backticks you type from trust 55.
        assert!(!Who::from_packets(None, None).keeps_backticks);
        assert!(!Who::from_packets(Some(54), None).keeps_backticks);
        assert!(Who::from_packets(Some(55), None).keeps_backticks);
    }
}
