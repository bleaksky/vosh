//! What Vosh knows about Aabahran alone.
//!
//! The PROMPT code compiler lands here in a later phase. For now it holds
//! the lamented tears rule (H7 in section 1.2 of the build spec). The
//! older server build sends true values under the song, and only
//! Char.Affects naming it tells Vosh the game means to hide them.

use crate::gmcp::Affects;

/// The song that hides your vitals, affects, group and your opponent's
/// condition while it is on you.
pub const LAMENT: &str = "lamented tears";

/// True when Char.Affects names lamented tears, in any case.
pub fn names_lament(affects: &Affects) -> bool {
    affects
        .list
        .iter()
        .any(|a| a.name.trim().eq_ignore_ascii_case(LAMENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gmcp::Affect;

    fn affects(names: &[&str]) -> Affects {
        Affects {
            list: names
                .iter()
                .map(|n| Affect {
                    name: (*n).to_string(),
                    ..Affect::default()
                })
                .collect(),
            hidden: false,
        }
    }

    #[test]
    fn the_song_is_found_by_name_in_any_case() {
        assert!(names_lament(&affects(&["bless", "lamented tears"])));
        assert!(names_lament(&affects(&["Lamented Tears"])));
        assert!(!names_lament(&affects(&["bless", "tears"])));
        assert!(!names_lament(&affects(&[])));
    }
}
