//! Keep the colors triggers paint text in readable on the terminal ground.
//!
//! A trigger can paint text a fixed color: a `{#8fa7d9}` token in a Replace
//! template goes out as `38;2;143;167;217`, and a `{fg:244}` token goes out
//! as `38;5;244`, a 256 color index past the 16 that draws the same on every
//! theme. A fixed color reads on some grounds and fades on others. #8fa7d9
//! holds about 5:1 on Nord and drops to about 2.2:1 on Vellum.
//! [`lift_to_contrast`] measures a color against the ground it lands on and,
//! when it falls short of [`READABLE_CONTRAST`], moves only its lightness
//! until it reads, darker on a light ground and lighter on a dark one. Its
//! OKLCH hue holds, and its chroma drops only where the sRGB gamut forces
//! it. A color that already reads comes back as it was.
//!
//! [`lift_sgr`] applies that to the escapes in a line the triggers built.
//! It changes fixed text and underline colors and never a background or one
//! of the theme's 16 colors. A 256 color that needs a lift goes out as a
//! true color. The engine runs it on a line a trigger rebuilt, whose escapes
//! all came from triggers, and on the open of each highlight and base color
//! it draws over the bytes the game sent, so the colors the game sends never
//! reach it.
//!
//! The game's own 256 colors get a floor of their own. On a light ground
//! [`lift_game_sgr`] darkens a 256 color past the 16 that the game sends
//! text in until it reads at [`GAME_LC`], by the same move in lightness
//! alone. Aabahran's minimap draws desert in yellow 220 and snow in white
//! 255, which read at Lc 0 on a parchment ground. A dark ground keeps the
//! game's colors as sent.

use std::borrow::Cow;
use std::ops::Range;

use crate::trigger::color::{wash_field, NamedColor};

/// An sRGB color, one byte per channel.
pub type Rgb = (u8, u8, u8);

/// The contrast a trigger color must reach on its ground, the WCAG floor
/// for body text and the floor the chrome gives words drawn in a status
/// color.
pub const READABLE_CONTRAST: f64 = 4.5;

/// The APCA lightness contrast a 256 color the game sends text in must
/// reach on a light ground. Lc 30 is APCA's least contrast for text of
/// any kind, so a map glyph or a prompt tag still shows.
pub const GAME_LC: f64 = 30.0;

/// The Oklab lightness under which a ground counts as dark, the page's
/// `APPEARANCE_THRESHOLD` in src/theme/chrome.ts.
const DARK_GROUND_L: f64 = 0.6;

/// Read a `#rrggbb` or `#rgb` color, the leading `#` optional.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(hex.get(range)?, 16).ok();
    match hex.len() {
        6 => Some((channel(0..2)?, channel(2..4)?, channel(4..6)?)),
        3 => {
            let short = |i: usize| channel(i..i + 1).map(|n| n * 17);
            Some((short(0)?, short(1)?, short(2)?))
        }
        _ => None,
    }
}

/// The fixed color of a 256 color index past the 16, as xterm draws it:
/// the 6 by 6 by 6 cube from 16, then the gray ramp from 232. The first 16
/// are the theme's palette, which this crate does not know, so they are
/// `None`.
pub fn xterm256(n: u8) -> Option<Rgb> {
    match n {
        0..=15 => None,
        16..=231 => {
            let i = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            Some((level(i / 36), level(i / 6 % 6), level(i % 6)))
        }
        232..=255 => {
            let v = 8 + 10 * (n - 232);
            Some((v, v, v))
        }
    }
}

fn to_linear(c: u8) -> f64 {
    let v = f64::from(c) / 255.0;
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f64) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

/// WCAG 2 relative luminance.
pub fn luminance(c: Rgb) -> f64 {
    0.2126 * to_linear(c.0) + 0.7152 * to_linear(c.1) + 0.0722 * to_linear(c.2)
}

/// WCAG 2 contrast ratio, 1 to 21, the same either way round.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// APCA lightness contrast, Lc, of `text` on `ground`, by the constants of
/// APCA 0.0.98G-4g, the release the theme review read every Lc with. It
/// is positive for text darker than its ground and negative for text
/// lighter than it, from about 106 for black on white to about -108 for
/// white on black, and 0 for two colors too close for APCA to read apart.
pub fn apca_lc(text: Rgb, ground: Rgb) -> f64 {
    // Screen luminance on APCA's plain 2.4 power curve, with the darkest
    // colors eased up off black.
    let screen = |c: Rgb| {
        let channel = |v: u8| (f64::from(v) / 255.0).powf(2.4);
        let y = 0.212_672_9 * channel(c.0) + 0.715_152_2 * channel(c.1) + 0.072_175 * channel(c.2);
        if y < 0.022 {
            y + (0.022 - y).powf(1.414)
        } else {
            y
        }
    };
    let (text, ground) = (screen(text), screen(ground));
    if (ground - text).abs() < 0.0005 {
        return 0.0;
    }
    let lc = if ground > text {
        let s = (ground.powf(0.56) - text.powf(0.57)) * 1.14;
        if s < 0.1 {
            0.0
        } else {
            s - 0.027
        }
    } else {
        let s = (ground.powf(0.65) - text.powf(0.62)) * 1.14;
        if s > -0.1 {
            0.0
        } else {
            s + 0.027
        }
    };
    lc * 100.0
}

/// OKLCH coordinates, the hue in radians.
#[derive(Clone, Copy, Debug)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// A color's OKLCH coordinates, through Oklab.
pub fn oklch(rgb: Rgb) -> Oklch {
    let (red, green, blue) = (to_linear(rgb.0), to_linear(rgb.1), to_linear(rgb.2));
    let long = (0.412_221_470_8 * red + 0.536_332_536_3 * green + 0.051_445_992_9 * blue).cbrt();
    let medium = (0.211_903_498_2 * red + 0.680_699_545_1 * green + 0.107_396_956_6 * blue).cbrt();
    let short = (0.088_302_461_9 * red + 0.281_718_837_6 * green + 0.629_978_700_5 * blue).cbrt();
    let lightness = 0.210_454_255_3 * long + 0.793_617_785 * medium - 0.004_072_046_8 * short;
    let green_red = 1.977_998_495_1 * long - 2.428_592_205 * medium + 0.450_593_709_9 * short;
    let blue_yellow = 0.025_904_037_1 * long + 0.782_771_766_2 * medium - 0.808_675_766 * short;
    Oklch {
        l: lightness,
        c: green_red.hypot(blue_yellow),
        h: blue_yellow.atan2(green_red),
    }
}

/// Linear sRGB channels for OKLCH coordinates, not clamped, so a channel
/// outside 0 to 1 marks a color the gamut cannot show.
fn linear_of(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let (green_red, blue_yellow) = (chroma * hue.cos(), chroma * hue.sin());
    let long = (lightness + 0.396_337_777_4 * green_red + 0.215_803_757_3 * blue_yellow).powi(3);
    let medium = (lightness - 0.105_561_345_8 * green_red - 0.063_854_172_8 * blue_yellow).powi(3);
    let short = (lightness - 0.089_484_177_5 * green_red - 1.291_485_548 * blue_yellow).powi(3);
    [
        4.076_741_662_1 * long - 3.307_711_591_3 * medium + 0.230_969_929_2 * short,
        -1.268_438_004_6 * long + 2.609_757_401_1 * medium - 0.341_319_396_5 * short,
        -0.004_196_086_3 * long - 0.703_418_614_7 * medium + 1.707_614_701 * short,
    ]
}

fn in_gamut(linear: [f64; 3]) -> bool {
    linear.iter().all(|v| (-1e-6..=1.0 + 1e-6).contains(v))
}

/// Chroma under which a color counts as a gray. A gray's Oklab a and b
/// come out a hair off zero, and that hair would tint the gray it lifts to.
const GRAY_CHROMA: f64 = 1e-4;

/// The color at lightness `l` and hue `h` with as much of chroma `c` as
/// the sRGB gamut holds there.
fn at_lightness(l: f64, c: f64, h: f64) -> Rgb {
    if c < GRAY_CHROMA {
        let v = from_linear(l.clamp(0.0, 1.0).powi(3));
        return (v, v, v);
    }
    let mut linear = linear_of(l, c, h);
    if !in_gamut(linear) {
        let (mut fits, mut over) = (0.0, c);
        for _ in 0..24 {
            let mid = (fits + over) / 2.0;
            if in_gamut(linear_of(l, mid, h)) {
                fits = mid;
            } else {
                over = mid;
            }
        }
        linear = linear_of(l, fits, h);
    }
    (
        from_linear(linear[0]),
        from_linear(linear[1]),
        from_linear(linear[2]),
    )
}

/// `fg` moved in OKLCH lightness until it holds [`READABLE_CONTRAST`] on
/// `ground`, by the smallest move that does. It darkens on a light ground
/// and lightens on a dark one, keeps its hue, and gives up chroma only
/// where the gamut runs out. A color that already reads returns as it is.
pub fn lift_to_contrast(fg: Rgb, ground: Rgb) -> Rgb {
    if contrast(fg, ground) >= READABLE_CONTRAST {
        return fg;
    }
    let start = oklch(fg);
    let ends = if oklch(ground).l < DARK_GROUND_L {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    };
    let reads = |rgb: Rgb| contrast(rgb, ground) >= READABLE_CONTRAST;
    // Black or white holds 4.5:1 on any ground, since 4.5 squared is
    // under 21, so the second end always reads when the first does not.
    ends.into_iter()
        .find_map(|end| lift_toward(start, end, reads))
        .unwrap_or(fg)
}

/// The color at the lightness between `start` and `end` nearest `start`
/// that `reads`, with `start`'s hue and as much of its chroma as the gamut
/// holds there, or `None` when even `end` does not read.
fn lift_toward(start: Oklch, end: f64, reads: impl Fn(Rgb) -> bool) -> Option<Rgb> {
    let reading_at = |l: f64| {
        let rgb = at_lightness(l, start.c, start.h);
        reads(rgb).then_some(rgb)
    };
    let mut best = reading_at(end)?;
    let (mut short, mut far) = (start.l, end);
    for _ in 0..32 {
        let mid = (short + far) / 2.0;
        match reading_at(mid) {
            Some(rgb) => {
                best = rgb;
                far = mid;
            }
            None => short = mid,
        }
    }
    Some(best)
}

/// What text draws on, as far as the line's own escapes tell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Under {
    /// The terminal ground: no background.
    #[default]
    Ground,
    /// A wash, which carries one of the [`NamedColor::wash_tint`] signals,
    /// held here as the canonical color of its mark. Both renderers paint
    /// the field the tint signals, the ground moved [`wash_field`] toward
    /// the theme's color for the mark, and never the tint itself. The
    /// session knows the ground but not the theme's palette, so the
    /// canonical mark stands in for the theme's, close enough at 18
    /// percent off the ground.
    Wash(Rgb),
    /// A true color or a 256 color past the 16.
    Fixed(Rgb),
    /// One of the theme's 16 colors, which the session does not know.
    Palette,
}

/// A fixed color the triggers asked for and the one the line draws.
#[derive(Clone, Copy, Debug)]
struct Paint {
    asked: Rgb,
    drawn: Rgb,
}

/// Where a color sits in a sequence's parameters.
#[derive(Clone, Copy, Debug)]
enum At {
    /// `38;2;r;g;b`, the index of the red item.
    Semicolons(usize),
    /// `38:2::r:g:b` in one item, its index.
    Colons(usize),
    /// `38;5;n`, the index of the `38`. A lift rewrites it to `38;2;r;g;b`
    /// in the same items.
    IndexedSemicolons(usize),
    /// `38:5:n` in one item, its index. A lift rewrites it to
    /// `38:2::r:g:b`.
    IndexedColons(usize),
}

/// The attributes the line's escapes have set so far.
#[derive(Debug, Default)]
struct Pen {
    fg: Option<Paint>,
    underline: Option<Paint>,
    under: Under,
}

/// The two kinds of color [`lift_sgr`] lifts.
#[derive(Clone, Copy)]
enum Kind {
    Text,
    Underline,
}

/// The canonical color of the mark whose wash tint is `rgb`, if any.
fn wash_mark(rgb: Rgb) -> Option<Rgb> {
    NamedColor::ALL
        .iter()
        .find(|c| c.wash_tint() == rgb)
        .map(|c| c.rgb())
}

fn channels(r: &str, g: &str, b: &str) -> Option<Rgb> {
    Some((r.parse().ok()?, g.parse().ok()?, b.parse().ok()?))
}

impl Pen {
    /// Take one SGR sequence's parameters, and return them rewritten when
    /// a fixed color in force after it needs another lightness to read.
    fn sgr(&mut self, params: &str, ground: Rgb) -> Option<String> {
        let mut items: Vec<String> = params.split(';').map(str::to_string).collect();
        // Where this sequence sets each color still in force at its end.
        let mut set_text: Option<At> = None;
        let mut set_underline: Option<At> = None;
        let mut k = 0;
        while k < items.len() {
            if items[k].contains(':') {
                let subs: Vec<&str> = items[k].split(':').collect();
                let n = subs.len();
                match (subs[0], subs.get(1).copied()) {
                    (code @ ("38" | "48" | "58"), Some("2")) if n >= 5 => {
                        if let Some(rgb) = channels(subs[n - 3], subs[n - 2], subs[n - 1]) {
                            let at = Some(At::Colons(k));
                            match code {
                                "38" => (self.fg, set_text) = (Some(paint(rgb)), at),
                                "58" => (self.underline, set_underline) = (Some(paint(rgb)), at),
                                _ => self.under = fixed_under(rgb),
                            }
                        }
                    }
                    (code @ ("38" | "48" | "58"), Some("5")) => {
                        let index = subs.get(2).and_then(|s| s.parse().ok());
                        let fixed = index.and_then(xterm256).map(paint);
                        let at = fixed.map(|_| At::IndexedColons(k));
                        match code {
                            "38" => (self.fg, set_text) = (fixed, at),
                            "58" => (self.underline, set_underline) = (fixed, at),
                            _ => self.under = indexed_under(index),
                        }
                    }
                    _ => {}
                }
                k += 1;
                continue;
            }
            let code: u16 = if items[k].is_empty() {
                0
            } else if let Ok(code) = items[k].parse() {
                code
            } else {
                k += 1;
                continue;
            };
            match code {
                0 => {
                    *self = Pen::default();
                    (set_text, set_underline) = (None, None);
                }
                38 | 48 | 58 => match items.get(k + 1).map(String::as_str) {
                    Some("2") if k + 4 < items.len() => {
                        if let Some(rgb) = channels(&items[k + 2], &items[k + 3], &items[k + 4]) {
                            let at = Some(At::Semicolons(k + 2));
                            match code {
                                38 => (self.fg, set_text) = (Some(paint(rgb)), at),
                                58 => (self.underline, set_underline) = (Some(paint(rgb)), at),
                                _ => self.under = fixed_under(rgb),
                            }
                        }
                        k += 4;
                    }
                    Some("5") if k + 2 < items.len() => {
                        let index = items[k + 2].parse().ok();
                        let fixed = index.and_then(xterm256).map(paint);
                        let at = fixed.map(|_| At::IndexedSemicolons(k));
                        match code {
                            38 => (self.fg, set_text) = (fixed, at),
                            58 => (self.underline, set_underline) = (fixed, at),
                            _ => self.under = indexed_under(index),
                        }
                        k += 2;
                    }
                    _ => {}
                },
                30..=37 | 39 | 90..=97 => (self.fg, set_text) = (None, None),
                40..=47 | 100..=107 => self.under = Under::Palette,
                49 => self.under = Under::Ground,
                59 => (self.underline, set_underline) = (None, None),
                _ => {}
            }
            k += 1;
        }

        let on = |asked: Rgb| readable_on(asked, self.under, ground);
        let text = settle(&mut self.fg, set_text, on, Kind::Text, &mut items);
        let underline = settle(
            &mut self.underline,
            set_underline,
            on,
            Kind::Underline,
            &mut items,
        );
        (text || underline).then(|| items.join(";"))
    }
}

/// The color `asked` draws in on `under`, with `ground` the terminal
/// background.
fn readable_on(asked: Rgb, under: Under, ground: Rgb) -> Rgb {
    match under {
        Under::Ground => lift_to_contrast(asked, ground),
        Under::Fixed(rgb) => lift_to_contrast(asked, rgb),
        // The theme's own background, the theme's to keep readable.
        Under::Palette => asked,
        // Both renderers paint the field near the ground, so a lift has
        // to read on the ground and on the field. When the lift reads on
        // only one, the color asked for stays.
        Under::Wash(mark) => {
            let field = wash_field(mark, ground);
            let lifted = lift_to_contrast(lift_to_contrast(asked, ground), field);
            let reads = |on: Rgb| contrast(lifted, on) >= READABLE_CONTRAST;
            if reads(ground) && reads(field) {
                lifted
            } else {
                asked
            }
        }
    }
}

fn paint(rgb: Rgb) -> Paint {
    Paint {
        asked: rgb,
        drawn: rgb,
    }
}

fn fixed_under(rgb: Rgb) -> Under {
    wash_mark(rgb).map_or(Under::Fixed(rgb), Under::Wash)
}

fn indexed_under(index: Option<u8>) -> Under {
    index
        .and_then(xterm256)
        .map_or(Under::Palette, Under::Fixed)
}

/// Give a color in force the color `on` draws it in. A color this sequence
/// set is rewritten in place. One set earlier gets a new color appended
/// when the ground under it changed. True when the items changed.
fn settle(
    paint: &mut Option<Paint>,
    set_here: Option<At>,
    on: impl Fn(Rgb) -> Rgb,
    kind: Kind,
    items: &mut Vec<String>,
) -> bool {
    let Some(paint) = paint else {
        return false;
    };
    let want = on(paint.asked);
    let changed = match set_here {
        Some(_) if want == paint.asked => false,
        Some(at) => {
            write_at(items, at, want);
            true
        }
        None if want == paint.drawn => false,
        None => {
            let (r, g, b) = want;
            items.push(match kind {
                Kind::Text => format!("38;2;{r};{g};{b}"),
                Kind::Underline => format!("58:2::{r}:{g}:{b}"),
            });
            true
        }
    };
    paint.drawn = want;
    changed
}

/// Write `rgb` over the color at `at` in a sequence's items.
fn write_at(items: &mut [String], at: At, rgb: Rgb) {
    let (red, green, blue) = rgb;
    match at {
        At::Semicolons(i) => {
            items[i] = red.to_string();
            items[i + 1] = green.to_string();
            items[i + 2] = blue.to_string();
        }
        At::Colons(i) => {
            let mut subs: Vec<String> = items[i].split(':').map(str::to_string).collect();
            let first = subs.len() - 3;
            subs[first] = red.to_string();
            subs[first + 1] = green.to_string();
            subs[first + 2] = blue.to_string();
            items[i] = subs.join(":");
        }
        // The index becomes a true color in the same items: `38;5;n` turns
        // into `38;2;r;g;b`, the channels in one item that the join spells
        // out with semicolons.
        At::IndexedSemicolons(i) => {
            items[i + 1] = "2".to_string();
            items[i + 2] = format!("{red};{green};{blue}");
        }
        At::IndexedColons(i) => {
            let code = items[i].split(':').next().unwrap_or("38").to_string();
            items[i] = format!("{code}:2::{red}:{green}:{blue}");
        }
    }
}

/// Each plain SGR sequence in `bytes`, in order: the bytes it runs over,
/// from its ESC through its final `m`, and its parameters, which hold
/// digits, semicolons and colons alone. Every other escape is passed over
/// whole.
fn plain_sgrs(bytes: &[u8]) -> impl Iterator<Item = (Range<usize>, &str)> + '_ {
    let mut from = 0;
    std::iter::from_fn(move || loop {
        let start = from + bytes[from..].windows(2).position(|pair| pair == b"\x1b[")?;
        let mut end = start + 2;
        while end < bytes.len() && (0x30..=0x3f).contains(&bytes[end]) {
            end += 1;
        }
        let params_end = end;
        while end < bytes.len() && (0x20..=0x2f).contains(&bytes[end]) {
            end += 1;
        }
        // The final byte. A sequence the line cuts short ends the scan.
        if end >= bytes.len() {
            return None;
        }
        let params = &bytes[start + 2..params_end];
        let plain = bytes[end] == b'm'
            && params_end == end
            && params
                .iter()
                .all(|b| b.is_ascii_digit() || matches!(b, b';' | b':'));
        from = end + 1;
        // Digits, semicolons and colons are ASCII, so they always read as
        // text.
        if let (true, Ok(params)) = (plain, std::str::from_utf8(params)) {
            return Some((start..from, params));
        }
    })
}

/// Rewrite the fixed text and underline colors in `text`, a line the
/// triggers built, so each holds [`READABLE_CONTRAST`] on what it draws
/// on: `ground`, the terminal background, unless the line set a fixed
/// background of its own. A fixed color is a true color or a 256 color
/// past the 16, and a 256 color that needs a lift goes out as a true color.
/// The theme's 16 colors stay. Text on one of the theme's 16 background
/// colors keeps the color asked for, since that background is the theme's
/// to know, and text on a wash changes only to a color that reads under
/// both renderers. Backgrounds never change, and neither does anything but
/// SGR sequences. Borrows `text` back when nothing needed a change.
pub fn lift_sgr(text: &str, ground: Rgb) -> Cow<'_, str> {
    let mut pen = Pen::default();
    let mut out = String::new();
    // The end of the text copied into `out` so far. A sequence starts and
    // ends on ASCII, so each cut falls between characters.
    let mut copied = 0;
    for (at, params) in plain_sgrs(text.as_bytes()) {
        if let Some(rewritten) = pen.sgr(params, ground) {
            out.push_str(&text[copied..at.start]);
            out.push_str("\x1b[");
            out.push_str(&rewritten);
            out.push('m');
            copied = at.end;
        }
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
    Cow::Owned(out)
}

/// What the game's escapes have set so far that takes its text off the
/// terminal ground.
#[derive(Debug, Default)]
struct OffGround {
    /// A background, from the theme's 16 or fixed.
    background: bool,
    /// Inverse video, which draws the text color behind the text.
    inverse: bool,
}

impl OffGround {
    /// Take one SGR sequence the game sent, and return its parameters
    /// rewritten when the 256 color it leaves text in fades on `ground`.
    fn sgr(&mut self, params: &str, ground: Rgb) -> Option<String> {
        let mut items: Vec<String> = params.split(';').map(str::to_string).collect();
        // The 256 color this sequence leaves text in, and where it sits.
        let mut text: Option<(At, u8)> = None;
        let mut k = 0;
        while k < items.len() {
            if items[k].contains(':') {
                let subs: Vec<&str> = items[k].split(':').collect();
                match (subs[0], subs.get(1).copied()) {
                    ("38", Some("5")) => {
                        let index = subs.get(2).and_then(|n| n.parse().ok());
                        text = index.map(|n| (At::IndexedColons(k), n));
                    }
                    ("38", _) => text = None,
                    ("48", _) => self.background = true,
                    _ => {}
                }
                k += 1;
                continue;
            }
            let code: u16 = if items[k].is_empty() {
                0
            } else if let Ok(code) = items[k].parse() {
                code
            } else {
                k += 1;
                continue;
            };
            match code {
                0 => {
                    *self = OffGround::default();
                    text = None;
                }
                38 | 48 | 58 => {
                    let args = match items.get(k + 1).map(String::as_str) {
                        Some("5") => 2,
                        Some("2") => 4,
                        _ => 0,
                    };
                    if code == 38 {
                        let index = items.get(k + 2).and_then(|n| n.parse().ok());
                        text = index
                            .filter(|_| args == 2)
                            .map(|n| (At::IndexedSemicolons(k), n));
                    } else if code == 48 {
                        self.background = true;
                    }
                    k += args;
                }
                30..=37 | 39 | 90..=97 => text = None,
                40..=47 | 100..=107 => self.background = true,
                49 => self.background = false,
                7 => self.inverse = true,
                27 => self.inverse = false,
                _ => {}
            }
            k += 1;
        }
        if self.background || self.inverse {
            return None;
        }
        let (at, index) = text?;
        let asked = xterm256(index)?;
        let reads = |rgb: Rgb| apca_lc(rgb, ground).abs() >= GAME_LC;
        if reads(asked) {
            return None;
        }
        // Black reads at about Lc 40 on a gray right at the dark ground
        // line, so the search finds a color on every light ground a theme
        // ships. Where it finds none, the color stays.
        let drawn = lift_toward(oklch(asked), 0.0, reads)?;
        write_at(&mut items, at, drawn);
        Some(items.join(";"))
    }
}

/// Rewrite the 256 colors past the 16 that the game sends text in, in
/// `line` as the game sent it, so each reads at [`GAME_LC`] on `ground`
/// when `ground` is light. A color under it moves only in OKLCH lightness,
/// darker, at its own hue, gives up chroma only where the sRGB gamut runs
/// out, and goes out as a true color. A color the line sets while a
/// background or inverse video is in force keeps its index, since it does
/// not draw on the ground. The theme's 16 colors, true colors, underline
/// colors and backgrounds never change. Borrows `line` back on a dark
/// ground, and when every color already reads.
pub fn lift_game_sgr(line: &[u8], ground: Rgb) -> Cow<'_, [u8]> {
    if oklch(ground).l < DARK_GROUND_L {
        return Cow::Borrowed(line);
    }
    let mut off = OffGround::default();
    let mut out = Vec::new();
    // The end of the line copied into `out` so far.
    let mut copied = 0;
    for (at, params) in plain_sgrs(line) {
        if let Some(rewritten) = off.sgr(params, ground) {
            out.extend_from_slice(&line[copied..at.start]);
            out.extend_from_slice(b"\x1b[");
            out.extend_from_slice(rewritten.as_bytes());
            out.push(b'm');
            copied = at.end;
        }
    }
    if copied == 0 {
        return Cow::Borrowed(line);
    }
    out.extend_from_slice(&line[copied..]);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEATHER: Rgb = (0x8f, 0xa7, 0xd9);
    const VELLUM: Rgb = (0xf7, 0xf4, 0xee);
    const NORD: Rgb = (0x2e, 0x34, 0x40);

    /// The shared fixture: every built in theme's terminal background, the
    /// fixed colors the presets and a true color highlight paint text in,
    /// and every Replace template of the presets.
    #[derive(serde::Deserialize)]
    struct Fixture {
        grounds: Vec<Ground>,
        colors: Colors,
        templates: Vec<String>,
    }

    #[derive(serde::Deserialize)]
    struct Ground {
        theme: String,
        background: String,
    }

    #[derive(serde::Deserialize)]
    struct Colors {
        true_color: Vec<String>,
        indexed: Vec<u8>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!("../../../../fixtures/readable/grounds.json")).unwrap()
    }

    /// Every color in the fixture as sRGB, with a name for messages.
    fn fixture_colors(f: &Fixture) -> Vec<(String, Rgb)> {
        let mut out = Vec::new();
        for hex in &f.colors.true_color {
            out.push((hex.clone(), parse_hex(hex).unwrap()));
        }
        for &n in &f.colors.indexed {
            out.push((format!("fg:{n}"), xterm256(n).unwrap()));
        }
        out
    }

    fn hue_gap(a: Oklch, b: Oklch) -> f64 {
        let gap = (a.h - b.h).abs().to_degrees() % 360.0;
        gap.min(360.0 - gap)
    }

    #[test]
    fn a_gray_lifts_to_a_gray() {
        // The presets' dark gray for routine parries, on Nord and Vellum.
        for ground in [NORD, VELLUM] {
            let (r, g, b) = lift_to_contrast((0x58, 0x58, 0x58), ground);
            assert!(r == g && g == b, "#{r:02x}{g:02x}{b:02x} on {ground:?}");
        }
        assert_eq!(
            lift_to_contrast((0xda, 0xda, 0xda), VELLUM),
            (0x70, 0x70, 0x70)
        );
    }

    #[test]
    fn parses_hex_forms() {
        assert_eq!(parse_hex("#8fa7d9"), Some(WEATHER));
        assert_eq!(parse_hex("8FA7D9"), Some(WEATHER));
        assert_eq!(parse_hex(" #f80 "), Some((0xff, 0x88, 0x00)));
        assert_eq!(parse_hex("#8fa7d"), None);
        assert_eq!(parse_hex("#8fa7dz"), None);
        assert_eq!(parse_hex("rgb(1, 2, 3)"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn xterm256_matches_the_xterm_chart() {
        assert_eq!(xterm256(15), None);
        assert_eq!(xterm256(16), Some((0, 0, 0)));
        assert_eq!(xterm256(178), Some((0xd7, 0xaf, 0x00)));
        assert_eq!(xterm256(231), Some((0xff, 0xff, 0xff)));
        assert_eq!(xterm256(232), Some((8, 8, 8)));
        assert_eq!(xterm256(240), Some((0x58, 0x58, 0x58)));
        assert_eq!(xterm256(255), Some((0xee, 0xee, 0xee)));
    }

    #[test]
    fn contrast_matches_wcag() {
        assert!((contrast((0, 0, 0), (255, 255, 255)) - 21.0).abs() < 1e-9);
        assert!((contrast(WEATHER, WEATHER) - 1.0).abs() < 1e-9);
        // The figures the readable switch was offered on.
        assert!((contrast(WEATHER, VELLUM) - 2.2).abs() < 0.1);
        assert!(contrast(WEATHER, NORD) > READABLE_CONTRAST);
    }

    #[test]
    fn weather_blue_darkens_on_vellum_and_holds_on_nord() {
        let lifted = lift_to_contrast(WEATHER, VELLUM);
        assert_ne!(lifted, WEATHER);
        assert!(contrast(lifted, VELLUM) >= READABLE_CONTRAST);
        assert!(oklch(lifted).l < oklch(WEATHER).l);
        assert!(hue_gap(oklch(lifted), oklch(WEATHER)) < 2.0);
        // The smallest move: one step less dark no longer reads.
        assert!(contrast(lifted, VELLUM) < READABLE_CONTRAST + 0.1);
        assert_eq!(lift_to_contrast(WEATHER, NORD), WEATHER);
    }

    #[test]
    fn weather_blue_darkens_on_the_solarized_light_ground() {
        // Solarized Light and Everforest Light share this paper.
        let paper = (0xfd, 0xf6, 0xe3);
        assert!(contrast(WEATHER, paper) < 2.3);
        let lifted = lift_to_contrast(WEATHER, paper);
        assert!(contrast(lifted, paper) >= READABLE_CONTRAST);
        assert!(hue_gap(oklch(lifted), oklch(WEATHER)) < 2.0);
    }

    #[test]
    fn a_dim_color_lightens_on_a_dark_ground() {
        let dim = (0x30, 0x40, 0x80);
        let lifted = lift_to_contrast(dim, NORD);
        assert!(contrast(lifted, NORD) >= READABLE_CONTRAST);
        assert!(oklch(lifted).l > oklch(dim).l);
        assert!(hue_gap(oklch(lifted), oklch(dim)) < 2.0);
    }

    #[test]
    fn a_vivid_color_gives_up_chroma_only_where_the_gamut_ends() {
        // Pure blue on a deep blue ground has to come far up in lightness,
        // where sRGB holds less blue chroma, and keeps its hue anyway.
        let blue = (0, 0, 0xff);
        let ground = (0x00, 0x2b, 0x36);
        let lifted = lift_to_contrast(blue, ground);
        assert!(contrast(lifted, ground) >= READABLE_CONTRAST);
        assert!(oklch(lifted).c < oklch(blue).c);
        assert!(hue_gap(oklch(lifted), oklch(blue)) < 2.0);
    }

    #[test]
    fn every_fixture_color_reads_on_every_built_in_ground() {
        let f = fixture();
        assert!(
            f.grounds.len() >= 15,
            "the fixture lists every built in theme"
        );
        let colors = fixture_colors(&f);
        for ground in &f.grounds {
            let bg = parse_hex(&ground.background).unwrap();
            let dark = oklch(bg).l < DARK_GROUND_L;
            for (name, fg) in &colors {
                let out = lift_to_contrast(*fg, bg);
                let at = format!("{name} on {} ({})", ground.theme, ground.background);
                assert!(
                    contrast(out, bg) >= READABLE_CONTRAST,
                    "{at} reads {:.2}:1",
                    contrast(out, bg)
                );
                if contrast(*fg, bg) >= READABLE_CONTRAST {
                    assert_eq!(out, *fg, "{at} already reads and stays");
                    continue;
                }
                let (was, now) = (oklch(*fg), oklch(out));
                if dark {
                    assert!(now.l > was.l, "{at} lightens on a dark ground");
                } else {
                    assert!(now.l < was.l, "{at} darkens on a light ground");
                }
                // A gray has no hue to keep, and a color near black or
                // white keeps too little chroma for its hue to show.
                if was.c > 0.03 && now.c > 0.03 {
                    assert!(hue_gap(was, now) < 3.0, "{at} keeps its hue");
                }
                // Rounding to bytes can add a hair of chroma.
                assert!(now.c <= was.c + 0.005, "{at} gains no chroma");
            }
        }
    }

    /// A store with one Replace trigger that saves `template` and matches a
    /// line of four words, so `$0` to `$4` all fill.
    fn replace_store(template: &str) -> crate::trigger::TriggerStore {
        let mut store = crate::trigger::TriggerStore::new();
        store
            .set(crate::trigger::Trigger::new(
                "preset",
                r"^(\S+) (\S+) (\S+) (\S+)$",
                crate::trigger::TriggerAction::Replace {
                    template: template.into(),
                },
            ))
            .unwrap();
        store
    }

    /// Each run of visible text in `line` that draws in a fixed color, with
    /// that color. The SGR model in the vosh-protocol test kit reads the line,
    /// so the check does not lean on the scan it checks.
    fn fixed_runs(line: &str) -> Vec<(String, Rgb)> {
        vosh_protocol::ansi::AnsiParser::new()
            .feed(line.as_bytes())
            .into_iter()
            .filter_map(|span| {
                let rgb = match span.attrs.fg {
                    vosh_protocol::ansi::Color::Rgb { r, g, b } => Some((r, g, b)),
                    vosh_protocol::ansi::Color::Indexed256(n) => xterm256(n),
                    _ => None,
                }?;
                (!span.text.trim().is_empty()).then_some((span.text, rgb))
            })
            .collect()
    }

    #[test]
    fn every_preset_template_draws_text_that_reads_on_every_built_in_ground() {
        let f = fixture();
        assert!(
            f.templates.len() >= 40,
            "the fixture lists every preset template"
        );
        let line = b"alpha beta gamma delta";
        let plain = vosh_protocol::ansi::plain_text(line);
        for template in &f.templates {
            let store = replace_store(template);
            let draw = |ground| {
                crate::trigger::process_on_ground(
                    &store,
                    line,
                    &plain,
                    crate::trigger::MatchScope::Line,
                    ground,
                    None,
                    crate::StopKey(1),
                )
                .display
                .unwrap()
            };
            let as_set = draw(None);
            assert!(
                !fixed_runs(&as_set).is_empty(),
                "{template:?} paints a fixed color"
            );
            let mut lifted = false;
            for ground in &f.grounds {
                let bg = parse_hex(&ground.background).unwrap();
                let out = draw(Some(bg));
                assert_eq!(
                    vosh_protocol::ansi::plain_text(out.as_bytes()),
                    vosh_protocol::ansi::plain_text(as_set.as_bytes()),
                    "{template:?} on {} keeps its text",
                    ground.theme
                );
                for (text, rgb) in fixed_runs(&out) {
                    assert!(
                        contrast(rgb, bg) >= READABLE_CONTRAST,
                        "{text:?} from {template:?} on {} reads {:.2}:1",
                        ground.theme,
                        contrast(rgb, bg)
                    );
                }
                lifted |= out != as_set;
            }
            // Every preset color fades on some built in ground, the light
            // ones on Vellum and the dark gray 240 on the darkest grounds,
            // so the check above saw a lift for every template.
            assert!(lifted, "{template:?} needs a lift on some ground");
        }
    }

    #[test]
    fn the_weather_blue_is_lifted_on_light_grounds_alone() {
        let f = fixture();
        for ground in &f.grounds {
            let bg = parse_hex(&ground.background).unwrap();
            let lifted = lift_to_contrast(WEATHER, bg) != WEATHER;
            assert_eq!(lifted, oklch(bg).l >= DARK_GROUND_L, "{}", ground.theme);
        }
    }

    #[test]
    fn a_line_without_escapes_is_borrowed() {
        assert!(matches!(lift_sgr("plain text", VELLUM), Cow::Borrowed(_)));
    }

    #[test]
    fn true_color_text_is_rewritten_in_place() {
        let line = "\x1b[38;2;143;167;217mIt starts to rain.\x1b[0m";
        let out = lift_sgr(line, VELLUM);
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            out,
            format!("\x1b[38;2;{r};{g};{b}mIt starts to rain.\x1b[0m")
        );
        assert_eq!(lift_sgr(line, NORD), line);
    }

    #[test]
    fn other_attributes_in_the_sequence_stay() {
        let line = "\x1b[1;4;38;2;143;167;217;3mrain\x1b[0m";
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            lift_sgr(line, VELLUM),
            format!("\x1b[1;4;38;2;{r};{g};{b};3mrain\x1b[0m")
        );
    }

    #[test]
    fn colon_forms_are_rewritten_too() {
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            lift_sgr("\x1b[38:2::143:167:217mrain", VELLUM),
            format!("\x1b[38:2::{r}:{g}:{b}mrain")
        );
        assert_eq!(
            lift_sgr("\x1b[38:2:143:167:217mrain", VELLUM),
            format!("\x1b[38:2:{r}:{g}:{b}mrain")
        );
    }

    #[test]
    fn an_underline_color_is_lifted_like_text() {
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            lift_sgr("\x1b[4;58:2::143:167:217mrain", VELLUM),
            format!("\x1b[4;58:2::{r}:{g}:{b}mrain")
        );
        assert_eq!(
            lift_sgr("\x1b[4;58;2;143;167;217mrain", VELLUM),
            format!("\x1b[4;58;2;{r};{g};{b}mrain")
        );
    }

    #[test]
    fn a_background_never_changes() {
        // A faint background alone stays, with nothing on it to lift.
        let line = "\x1b[48;2;240;240;240mrain\x1b[0m";
        assert_eq!(lift_sgr(line, VELLUM), line);
    }

    #[test]
    fn text_on_a_fixed_background_reads_on_that_background() {
        // Weather blue on white: the white is what it must read on, and the
        // white stays white.
        let line = "\x1b[48;2;255;255;255;38;2;143;167;217mrain";
        let (r, g, b) = lift_to_contrast(WEATHER, (255, 255, 255));
        assert_eq!(
            lift_sgr(line, NORD),
            format!("\x1b[48;2;255;255;255;38;2;{r};{g};{b}mrain")
        );
    }

    #[test]
    fn a_later_background_gets_the_text_color_again() {
        // The text color comes first, then a white background in its own
        // sequence: the color is lifted for the ground, then once more for
        // the white.
        let line = "\x1b[38;2;143;167;217mdry \x1b[48;2;255;255;255mwet\x1b[49m dry";
        let out = lift_sgr(line, NORD);
        let (r, g, b) = lift_to_contrast(WEATHER, (255, 255, 255));
        assert_eq!(
            out,
            format!(
                "\x1b[38;2;143;167;217mdry \x1b[48;2;255;255;255;38;2;{r};{g};{b}mwet\x1b[49;38;2;143;167;217m dry"
            )
        );
    }

    /// `color` as text on a yellow wash.
    fn on_a_wash(color: Rgb) -> String {
        let (tr, tg, tb) = NamedColor::Yellow.wash_tint();
        let (r, g, b) = color;
        format!("\x1b[33;48;2;{tr};{tg};{tb}m\x1b[38;2;{r};{g};{b}mrain\x1b[0m")
    }

    /// The field both renderers paint for a yellow wash on `ground`.
    fn yellow_field(ground: Rgb) -> Rgb {
        wash_field(NamedColor::Yellow.rgb(), ground)
    }

    #[test]
    fn a_wash_keeps_the_color_asked_for_when_it_reads_on_the_ground_and_the_field() {
        // On Nord a pale blue reads on the ground and on the yellow field
        // alike, so it stays.
        let pale = (0xc0, 0xd0, 0xf0);
        let line = on_a_wash(pale);
        assert!(contrast(pale, NORD) >= READABLE_CONTRAST);
        assert!(contrast(pale, yellow_field(NORD)) >= READABLE_CONTRAST);
        assert_eq!(lift_sgr(&line, NORD), line);
    }

    #[test]
    fn a_wash_lifts_a_color_to_read_on_the_ground_and_the_field() {
        // The weather blue fades on Vellum and on the pale yellow field
        // over it, and a dim blue fades on Nord and its field. Each lift
        // reads on both, since both renderers paint the field.
        let wash = NamedColor::Yellow.rgb();
        for (color, ground) in [(WEATHER, VELLUM), ((0x30, 0x40, 0x80), NORD)] {
            let line = on_a_wash(color);
            let field = yellow_field(ground);
            let (r, g, b) = readable_on(color, Under::Wash(wash), ground);
            assert_ne!((r, g, b), color);
            assert!(contrast((r, g, b), ground) >= READABLE_CONTRAST);
            assert!(contrast((r, g, b), field) >= READABLE_CONTRAST);
            let (cr, cg, cb) = color;
            assert_eq!(
                lift_sgr(&line, ground),
                line.replace(&format!("{cr};{cg};{cb}m"), &format!("{r};{g};{b}m"))
            );
        }
    }

    #[test]
    fn text_on_a_palette_background_keeps_its_color() {
        let line = "\x1b[44;38;2;143;167;217mrain\x1b[0m";
        assert_eq!(lift_sgr(line, VELLUM), line);
    }

    #[test]
    fn a_256_color_past_the_16_lifts_and_the_16_stay() {
        // 253 is a fixed light gray, about 1.3:1 on Vellum. 93 is the
        // theme's bright yellow, the theme's to keep readable.
        let line = "\x1b[38;5;253mrain\x1b[0m \x1b[93mbolt\x1b[0m \x1b[38;5;11mzap";
        let gray = xterm256(253).unwrap();
        assert!(contrast(gray, VELLUM) < 1.5);
        let (r, g, b) = lift_to_contrast(gray, VELLUM);
        assert_eq!(
            lift_sgr(line, VELLUM),
            format!("\x1b[38;2;{r};{g};{b}mrain\x1b[0m \x1b[93mbolt\x1b[0m \x1b[38;5;11mzap")
        );
        // On Nord the gray reads, and the index stays as it was.
        assert_eq!(lift_sgr(line, NORD), line);
    }

    #[test]
    fn a_256_color_lifts_beside_other_attributes_and_in_colon_form() {
        let gray = xterm256(253).unwrap();
        let (r, g, b) = lift_to_contrast(gray, VELLUM);
        assert_eq!(
            lift_sgr("\x1b[1;38;5;253;4mrain", VELLUM),
            format!("\x1b[1;38;2;{r};{g};{b};4mrain")
        );
        assert_eq!(
            lift_sgr("\x1b[38:5:253mrain", VELLUM),
            format!("\x1b[38:2::{r}:{g}:{b}mrain")
        );
        assert_eq!(
            lift_sgr("\x1b[4;58;5;253mrain", VELLUM),
            format!("\x1b[4;58;2;{r};{g};{b}mrain")
        );
        assert_eq!(
            lift_sgr("\x1b[4;58:5:253mrain", VELLUM),
            format!("\x1b[4;58:2::{r}:{g}:{b}mrain")
        );
    }

    #[test]
    fn a_256_color_set_earlier_lifts_again_on_a_later_background() {
        // 240 reads on Vellum. On a black background it fades, so the line
        // gets it again as a true color that reads there.
        let line = "\x1b[38;5;240mdry \x1b[48;5;16mwet\x1b[49m dry";
        let (r, g, b) = lift_to_contrast(xterm256(240).unwrap(), (0, 0, 0));
        assert_eq!(
            lift_sgr(line, VELLUM),
            format!(
                "\x1b[38;5;240mdry \x1b[48;5;16;38;2;{r};{g};{b}mwet\x1b[49;38;2;88;88;88m dry"
            )
        );
    }

    #[test]
    fn a_reset_ends_the_color() {
        // After the reset a background change has no text color to redo.
        let line = "\x1b[38;2;143;167;217mrain\x1b[0m \x1b[48;2;255;255;255msun";
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            lift_sgr(line, VELLUM),
            format!("\x1b[38;2;{r};{g};{b}mrain\x1b[0m \x1b[48;2;255;255;255msun")
        );
    }

    #[test]
    fn a_sequence_ending_past_ascii_passes_through() {
        // No terminal reads this as SGR. The scan steps over it whole and
        // still lifts the color after it.
        assert_eq!(lift_sgr("\x1b[1\u{e9}x", VELLUM), "\x1b[1\u{e9}x");
        assert_eq!(lift_sgr("\x1b[1;2 \u{e9}x", VELLUM), "\x1b[1;2 \u{e9}x");
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            lift_sgr("\x1b[1\u{e9}\x1b[38;2;143;167;217mrain", VELLUM),
            format!("\x1b[1\u{e9}\x1b[38;2;{r};{g};{b}mrain")
        );
    }

    #[test]
    fn other_escapes_pass_through() {
        let line = "\x1b[2K\x1b[?25l\x1b[38;2;143;167;217mrain\x1b[";
        let (r, g, b) = lift_to_contrast(WEATHER, VELLUM);
        assert_eq!(
            lift_sgr(line, VELLUM),
            format!("\x1b[2K\x1b[?25l\x1b[38;2;{r};{g};{b}mrain\x1b[")
        );
    }

    const RUBRIC: Rgb = (0xf0, 0xe5, 0xcf);
    const MELANGE_LIGHT: Rgb = (0xf1, 0xf1, 0xf1);
    const TRIAD: Rgb = (0x15, 0x0c, 0x22);

    /// The fixed colors Aabahran sends text in: 240, the Wizi and Incog
    /// tags before the prompt, then the minimap's sector colors from
    /// minimap.c, and 213, the @ that marks you on it.
    const GAME_FIXED: [u8; 15] = [
        240, 249, 180, 77, 34, 143, 241, 75, 33, 58, 117, 220, 196, 255, 213,
    ];

    /// `rgb` at the lightness nearest it that reads at [`GAME_LC`] on
    /// `ground`, darker.
    fn game_floor(rgb: Rgb, ground: Rgb) -> Rgb {
        lift_toward(oklch(rgb), 0.0, |c| apca_lc(c, ground).abs() >= GAME_LC).unwrap()
    }

    #[test]
    fn apca_lc_matches_the_reference_and_the_review() {
        // APCA's own sample pair, #888 on white and white on #888, and
        // its ends.
        let gray = (0x88, 0x88, 0x88);
        assert!((apca_lc(gray, (255, 255, 255)) - 63.056).abs() < 0.001);
        assert!((apca_lc((255, 255, 255), gray) + 68.541).abs() < 0.001);
        assert!((apca_lc((0, 0, 0), (255, 255, 255)) - 106.041).abs() < 0.001);
        assert!((apca_lc((255, 255, 255), (0, 0, 0)) + 107.885).abs() < 0.001);
        // The figures the review read: gray 249 at Lc 26.8 on Rubric, the
        // Wizi tag at -16.8 on Triad, and desert yellow at 0 on Rubric.
        assert!((apca_lc(xterm256(249).unwrap(), RUBRIC) - 26.783).abs() < 0.001);
        assert!((apca_lc(xterm256(240).unwrap(), TRIAD) + 16.765).abs() < 0.001);
        assert!(apca_lc(xterm256(220).unwrap(), RUBRIC).abs() < 1e-9);
    }

    /// The game's fixed colors as minimap glyphs on one line, each its
    /// own run, lifted for `ground`, with the indices that changed. Every
    /// glyph reads at [`GAME_LC`] afterwards, and each one that changed
    /// went darker at its own hue.
    fn minimap_on(ground: Rgb) -> Vec<u8> {
        let line = GAME_FIXED.map(|n| format!("\x1b[38;5;{n}m+")).concat();
        let out = lift_game_sgr(line.as_bytes(), ground);
        let runs = fixed_runs(std::str::from_utf8(&out).unwrap());
        assert_eq!(runs.len(), GAME_FIXED.len());
        let mut lifted = Vec::new();
        for (&n, (_, now)) in GAME_FIXED.iter().zip(runs) {
            let was = xterm256(n).unwrap();
            let lc = apca_lc(now, ground);
            assert!(lc.abs() >= GAME_LC, "{n} reads at Lc {lc:.1}");
            if now == was {
                continue;
            }
            lifted.push(n);
            let (was, now) = (oklch(was), oklch(now));
            assert!(now.l < was.l, "{n} darkens");
            if was.c < 1e-3 {
                assert!(now.c < 1e-3, "{n} stays gray");
            } else {
                assert!(hue_gap(was, now) < 2.0, "{n} keeps its hue");
                assert!(now.c <= was.c + 0.005, "{n} gains no chroma");
            }
        }
        lifted
    }

    #[test]
    fn seven_game_colors_lift_on_rubric() {
        assert_eq!(minimap_on(RUBRIC), [249, 180, 77, 117, 220, 255, 213]);
    }

    #[test]
    fn four_game_colors_lift_on_melange_light() {
        assert_eq!(minimap_on(MELANGE_LIGHT), [77, 117, 220, 255]);
    }

    #[test]
    fn a_dark_ground_keeps_every_game_color() {
        // On Triad the Wizi tag, gray 241 and olive 58 sit under Lc 30,
        // and they stay as the game sends them.
        let under: Vec<u8> = GAME_FIXED
            .into_iter()
            .filter(|&n| apca_lc(xterm256(n).unwrap(), TRIAD).abs() < GAME_LC)
            .collect();
        assert_eq!(under, [240, 241, 58]);
        for n in 16..=255u8 {
            let line = format!("\x1b[38;5;{n}m+");
            assert!(matches!(
                lift_game_sgr(line.as_bytes(), TRIAD),
                Cow::Borrowed(_)
            ));
        }
    }

    #[test]
    fn a_game_color_lifts_in_either_form_beside_other_attributes() {
        let (r, g, b) = game_floor(xterm256(220).unwrap(), RUBRIC);
        let lift = |line: &str| {
            String::from_utf8(lift_game_sgr(line.as_bytes(), RUBRIC).into_owned()).unwrap()
        };
        assert_eq!(
            lift("\x1b[1;38;5;220;4m.\x1b[0m"),
            format!("\x1b[1;38;2;{r};{g};{b};4m.\x1b[0m")
        );
        assert_eq!(lift("\x1b[38:5:220m."), format!("\x1b[38:2::{r}:{g}:{b}m."));
        // The tint the game sends before a room name, which the name's own
        // color then replaces, lifts alone.
        let (r, g, b) = game_floor(xterm256(255).unwrap(), RUBRIC);
        assert_eq!(
            lift("\x1b[38;5;255m\x1b[0;1;30mBefore the Temple of Neutrality\x1b[0;0m"),
            format!("\x1b[38;2;{r};{g};{b}m\x1b[0;1;30mBefore the Temple of Neutrality\x1b[0;0m")
        );
    }

    #[test]
    fn palette_true_underline_and_background_colors_stay() {
        for line in [
            "\x1b[38;5;11m+\x1b[93m+\x1b[38;5;3m+",
            "\x1b[38;2;255;215;0m+\x1b[38:2::238:238:238m+",
            "\x1b[4;58;5;220m+\x1b[4;58:5:255m+",
            "\x1b[48;5;220m \x1b[48;2;238;238;238m ",
            "plain text",
        ] {
            assert!(
                matches!(lift_game_sgr(line.as_bytes(), RUBRIC), Cow::Borrowed(_)),
                "{line:?}"
            );
        }
    }

    #[test]
    fn a_game_color_off_the_ground_keeps_its_index() {
        // Text on a background or in inverse video does not draw on the
        // ground, so it keeps the color the game sent.
        for line in [
            "\x1b[48;5;19;38;5;255m+",
            "\x1b[44m\x1b[38;5;255m+",
            "\x1b[48:5:19m\x1b[38:5:255m+",
            "\x1b[7;38;5;255m+",
        ] {
            assert!(
                matches!(lift_game_sgr(line.as_bytes(), RUBRIC), Cow::Borrowed(_)),
                "{line:?}"
            );
        }
        // A color the line sets once the background or the inverse ends
        // lifts.
        let (r, g, b) = game_floor(xterm256(255).unwrap(), RUBRIC);
        for (line, want) in [
            (
                "\x1b[44m \x1b[0m\x1b[38;5;255m+",
                "\x1b[44m \x1b[0m\x1b[38;2;{}m+",
            ),
            ("\x1b[44m \x1b[49;38;5;255m+", "\x1b[44m \x1b[49;38;2;{}m+"),
            ("\x1b[7m \x1b[27;38;5;255m+", "\x1b[7m \x1b[27;38;2;{}m+"),
        ] {
            assert_eq!(
                lift_game_sgr(line.as_bytes(), RUBRIC).as_ref(),
                want.replace("{}", &format!("{r};{g};{b}")).as_bytes(),
                "{line:?}"
            );
        }
    }
}
