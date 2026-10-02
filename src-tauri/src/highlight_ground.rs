//! The ground the colors your triggers paint text in must read on.
//!
//! While Keep highlight colors readable is on, the page reports the
//! theme's terminal background here (`highlight_ground_set`) on every
//! theme change, and the session hands it to the trigger engine with each
//! line it runs (`vosh_trigger::process_on_ground`). The engine lifts each
//! fixed color a trigger paints, a true color or a 256 color past the 16,
//! until it reads on that ground. While the switch is off, and before the
//! page reports, there is no ground, and trigger colors draw as you set
//! them. A theme change reaches the lines that arrive after it. Lines
//! already drawn keep their colors.

use std::sync::atomic::{AtomicU32, Ordering};

use vosh_trigger::readable::{self, Rgb};

/// The ground packed as `0x01_rr_gg_bb`, so black is still a ground, or 0
/// for none.
static GROUND: AtomicU32 = AtomicU32::new(0);

fn pack(ground: Option<Rgb>) -> u32 {
    ground.map_or(0, |(r, g, b)| {
        0x0100_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    })
}

fn unpack(bits: u32) -> Option<Rgb> {
    (bits != 0).then_some(((bits >> 16) as u8, (bits >> 8) as u8, bits as u8))
}

/// The ground trigger colors must read on, or `None` to draw them as set.
pub(crate) fn get() -> Option<Rgb> {
    unpack(GROUND.load(Ordering::Acquire))
}

/// Keep highlight colors readable. `background` is the theme's terminal
/// background as `#rrggbb` while the setting is on, and `None` while it is
/// off. A background that does not read turns lifting off too.
#[tauri::command]
pub(crate) fn highlight_ground_set(background: Option<String>) {
    let ground = background.as_deref().and_then(readable::parse_hex);
    GROUND.store(pack(ground), Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_keeps_every_ground_and_none() {
        for ground in [
            None,
            Some((0, 0, 0)),
            Some((0xf7, 0xf4, 0xee)),
            Some((255, 255, 255)),
        ] {
            assert_eq!(unpack(pack(ground)), ground);
        }
        assert_eq!(pack(None), 0);
        assert_ne!(pack(Some((0, 0, 0))), 0);
    }
}
