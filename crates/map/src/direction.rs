//! Direction normalization and coordinate offsets.
//!
//! MUDs send exit directions as either short forms (n, s, e, w, u, d) or
//! long forms (north, south, etc). This module maps both onto a single
//! canonical name and a 3D offset so the auto-positioner can place new
//! rooms relative to where the player came from.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
}

/// Resolve a direction string to its (canonical name, coordinate offset).
/// Returns `None` for non-cardinal exits like `enter`, `climb`, `portal`.
pub fn classify(input: &str) -> Option<(&'static str, Offset)> {
    let lower = input.to_ascii_lowercase();
    let lower = lower.trim();
    let (canonical, dx, dy, dz) = match lower {
        "n" | "north" => ("north", 0, -1, 0),
        "s" | "south" => ("south", 0, 1, 0),
        "e" | "east" => ("east", 1, 0, 0),
        "w" | "west" => ("west", -1, 0, 0),
        "ne" | "northeast" => ("northeast", 1, -1, 0),
        "nw" | "northwest" => ("northwest", -1, -1, 0),
        "se" | "southeast" => ("southeast", 1, 1, 0),
        "sw" | "southwest" => ("southwest", -1, 1, 0),
        "u" | "up" => ("up", 0, 0, 1),
        "d" | "down" => ("down", 0, 0, -1),
        _ => return None,
    };
    Some((canonical, Offset { dx, dy, dz }))
}

/// Reverse a canonical direction. Used to find the exit on the new room
/// that points back to the room we just came from.
pub fn reverse(canonical: &str) -> Option<&'static str> {
    Some(match canonical {
        "north" => "south",
        "south" => "north",
        "east" => "west",
        "west" => "east",
        "northeast" => "southwest",
        "northwest" => "southeast",
        "southeast" => "northwest",
        "southwest" => "northeast",
        "up" => "down",
        "down" => "up",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_and_short_forms_match() {
        assert_eq!(classify("n"), classify("north"));
        assert_eq!(classify("ne"), classify("northeast"));
        assert_eq!(classify("U"), classify("up"));
    }

    #[test]
    fn unknown_direction_is_none() {
        assert!(classify("enter").is_none());
        assert!(classify("portal").is_none());
    }

    #[test]
    fn reverse_pairs() {
        assert_eq!(reverse("north"), Some("south"));
        assert_eq!(reverse("up"), Some("down"));
        assert_eq!(reverse("northeast"), Some("southwest"));
        assert!(reverse("enter").is_none());
    }
}
