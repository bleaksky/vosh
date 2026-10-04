//! The grounds the colors in the text must read on.
//!
//! While Keep highlight colors readable is on, the page reports the
//! theme's terminal background here, through
//! `ipc::terminal::highlight_ground_set`, on every theme change, and the
//! session hands it to the trigger engine with each line it runs
//! (`vosh_automation::trigger::process_on_ground`). The engine lifts each
//! fixed color a trigger paints, a true color or a 256 color past the 16,
//! until it reads on that ground. While the switch is off, and before the
//! page reports, there is no ground, and trigger colors draw as you set
//! them.
//!
//! While Fit game colors is on, the page reports the same background as
//! the game ground, and the engine lifts the 256 colors past the 16 the
//! game sends text in, on a light ground, until they read. While it is
//! off there is no game ground, and the game's colors draw as sent.
//!
//! A theme change reaches the lines that arrive after it. Lines already
//! drawn keep their colors.

use std::sync::atomic::{AtomicU32, Ordering};

use vosh_automation::trigger::readable::Rgb;

/// The two grounds, each packed as `0x01_rr_gg_bb`, so black is still a
/// ground, or 0 for none.
struct Grounds {
    /// The one trigger colors read on.
    triggers: AtomicU32,
    /// The one the game's 256 colors read on.
    game: AtomicU32,
}

impl Grounds {
    const fn none() -> Self {
        Grounds {
            triggers: AtomicU32::new(0),
            game: AtomicU32::new(0),
        }
    }
}

#[cfg(not(test))]
static GROUNDS: Grounds = Grounds::none();

// Each test thread keeps its own, so a test that sets a ground leaves the
// lines the tests beside it run as they were.
#[cfg(test)]
thread_local! {
    static GROUNDS: Grounds = const { Grounds::none() };
}

#[cfg(not(test))]
fn with_grounds<R>(f: impl FnOnce(&Grounds) -> R) -> R {
    f(&GROUNDS)
}

#[cfg(test)]
fn with_grounds<R>(f: impl FnOnce(&Grounds) -> R) -> R {
    GROUNDS.with(f)
}

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
    with_grounds(|g| unpack(g.triggers.load(Ordering::Acquire)))
}

/// The ground the game's 256 colors must read on, or `None` to draw them
/// as sent.
pub(crate) fn game() -> Option<Rgb> {
    with_grounds(|g| unpack(g.game.load(Ordering::Acquire)))
}

/// Set both grounds for the lines that arrive from now on, `None` to draw
/// trigger colors as set or the game's colors as sent.
pub(crate) fn set(triggers: Option<Rgb>, game: Option<Rgb>) {
    with_grounds(|g| {
        g.triggers.store(pack(triggers), Ordering::Release);
        g.game.store(pack(game), Ordering::Release);
    });
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

    #[test]
    fn the_two_grounds_set_apart() {
        let paper = Some((0xf0, 0xe5, 0xcf));
        set(paper, None);
        assert_eq!((get(), game()), (paper, None));
        set(None, paper);
        assert_eq!((get(), game()), (None, paper));
        set(None, None);
    }
}
