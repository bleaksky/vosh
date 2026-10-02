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

use std::borrow::Cow;

use crate::trigger::color::NamedColor;

/// An sRGB color, one byte per channel.
pub type Rgb = (u8, u8, u8);

/// The contrast a trigger color must reach on its ground, the WCAG floor
/// for body text and the floor the chrome gives words drawn in a status
/// color.
pub const READABLE_CONTRAST: f64 = 4.5;

/// The Oklab lightness under which a ground counts as dark, the page's
/// `APPEARANCE_THRESHOLD` in src/lib/chrome.ts.
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
    // Black or white holds 4.5:1 on any ground, since 4.5 squared is
    // under 21, so the second end always reads when the first does not.
    ends.into_iter()
        .find_map(|end| lift_toward(start, end, ground))
        .unwrap_or(fg)
}

/// The lightness between `start` and `end` nearest `start` where the color
/// reads on `ground`, or `None` when even `end` does not.
fn lift_toward(start: Oklch, end: f64, ground: Rgb) -> Option<Rgb> {
    let reads = |l: f64| {
        let rgb = at_lightness(l, start.c, start.h);
        (contrast(rgb, ground) >= READABLE_CONTRAST).then_some(rgb)
    };
    let mut best = reads(end)?;
    let (mut short, mut far) = (start.l, end);
    for _ in 0..32 {
        let mid = (short + far) / 2.0;
        match reads(mid) {
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
    /// A wash, which carries one of the [`NamedColor::wash_tint`] signals.
    /// The two renderers draw it apart. The native one paints the field
    /// from the ground, 18 percent of the way toward the mark color, so the
    /// ground stands in for it there. xterm.js draws the tint itself, a
    /// quarter strength color such as #333300 for yellow.
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

fn is_wash(rgb: Rgb) -> bool {
    NamedColor::ALL.iter().any(|c| c.wash_tint() == rgb)
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
        // The same bytes go to both renderers, so a lift has to read on
        // the ground the native renderer paints and on the tint xterm.js
        // draws. When the lift reads on only one, the color asked for
        // stays. The tint is always dark, so on a light theme that is
        // nearly every color.
        Under::Wash(tint) => {
            let lifted = lift_to_contrast(lift_to_contrast(asked, ground), tint);
            let reads = |on: Rgb| contrast(lifted, on) >= READABLE_CONTRAST;
            if reads(ground) && reads(tint) {
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
    if is_wash(rgb) {
        Under::Wash(rgb)
    } else {
        Under::Fixed(rgb)
    }
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
        Some(At::Semicolons(i)) => {
            items[i] = want.0.to_string();
            items[i + 1] = want.1.to_string();
            items[i + 2] = want.2.to_string();
            true
        }
        Some(At::Colons(i)) => {
            let mut subs: Vec<String> = items[i].split(':').map(str::to_string).collect();
            let n = subs.len();
            subs[n - 3] = want.0.to_string();
            subs[n - 2] = want.1.to_string();
            subs[n - 1] = want.2.to_string();
            items[i] = subs.join(":");
            true
        }
        // The index becomes a true color in the same items: `38;5;n` turns
        // into `38;2;r;g;b`, the channels in one item that the join spells
        // out with semicolons.
        Some(At::IndexedSemicolons(i)) => {
            let (r, g, b) = want;
            items[i + 1] = "2".to_string();
            items[i + 2] = format!("{r};{g};{b}");
            true
        }
        Some(At::IndexedColons(i)) => {
            let (r, g, b) = want;
            let code = items[i].split(':').next().unwrap_or("38").to_string();
            items[i] = format!("{code}:2::{r}:{g}:{b}");
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
    if !text.contains("\x1b[") {
        return Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut pen = Pen::default();
    let mut out = String::new();
    // The end of the text copied into `out` so far.
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find("\x1b[") {
        let start = from + found;
        let mut end = start + 2;
        while end < bytes.len() && (0x30..=0x3f).contains(&bytes[end]) {
            end += 1;
        }
        let params_end = end;
        while end < bytes.len() && (0x20..=0x2f).contains(&bytes[end]) {
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        // A final byte past ASCII ends nothing a terminal reads as SGR, and
        // stepping one byte past it would land inside the character.
        if !bytes[end].is_ascii() {
            from = end;
            continue;
        }
        let params = &text[start + 2..params_end];
        let plain_sgr = bytes[end] == b'm'
            && params_end == end
            && params
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b';' || b == b':');
        end += 1;
        if plain_sgr {
            if let Some(rewritten) = pen.sgr(params, ground) {
                out.push_str(&text[copied..start]);
                out.push_str("\x1b[");
                out.push_str(&rewritten);
                out.push('m');
                copied = end;
            }
        }
        from = end;
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
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

    /// `color` as text on a yellow wash, and the wash tint.
    fn on_a_wash(color: Rgb) -> (String, Rgb) {
        let tint = NamedColor::Yellow.wash_tint();
        let ((tr, tg, tb), (r, g, b)) = (tint, color);
        let line = format!("\x1b[33;48;2;{tr};{tg};{tb}m\x1b[38;2;{r};{g};{b}mrain\x1b[0m");
        (line, tint)
    }

    #[test]
    fn a_wash_keeps_the_color_asked_for_when_no_lift_reads_under_both_renderers() {
        // xterm.js draws the tint itself, #333300, and the weather blue
        // reads there at about 5.4:1. The lift Vellum wants, #5a709e,
        // would fall to about 2.6:1 on it, so the blue stays.
        let (line, tint) = on_a_wash(WEATHER);
        assert_eq!(tint, (0x33, 0x33, 0x00));
        assert!(contrast(WEATHER, tint) >= READABLE_CONTRAST);
        assert!(contrast(lift_to_contrast(WEATHER, VELLUM), tint) < READABLE_CONTRAST);
        assert_eq!(lift_sgr(&line, VELLUM), line);
        // On a dark ground the blue reads on the ground and the tint alike.
        assert_eq!(lift_sgr(&line, NORD), line);
    }

    #[test]
    fn a_wash_lifts_a_color_to_read_on_the_ground_and_the_tint() {
        // A dim blue fades on Nord and on the tint. The lift reads on both,
        // since both renderers draw the same bytes.
        let dim = (0x30, 0x40, 0x80);
        let (line, tint) = on_a_wash(dim);
        let out = lift_sgr(&line, NORD);
        let (r, g, b) = readable_on(dim, Under::Wash(tint), NORD);
        assert_ne!((r, g, b), dim);
        assert!(contrast((r, g, b), NORD) >= READABLE_CONTRAST);
        assert!(contrast((r, g, b), tint) >= READABLE_CONTRAST);
        assert_eq!(out, line.replace("48;64;128", &format!("{r};{g};{b}")));
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
}
