//! The named ANSI 16 colors, the palette a highlight style draws from.
//! Each goes out as its 16 color SGR code, so the theme decides how it
//! looks.

use serde::{Deserialize, Serialize};

/// How far a washed row's field carries from the terminal ground toward
/// its mark color. Low enough that the row reads as marked rather than
/// painted.
pub const WASH_FIELD_MIX: f32 = 0.18;

/// The field a renderer paints behind a washed row, the ground moved
/// `WASH_FIELD_MIX` of the way toward the theme's color for the mark,
/// rounded per channel. The wash tint only signals which mark it is.
pub fn wash_field(mark: (u8, u8, u8), ground: (u8, u8, u8)) -> (u8, u8, u8) {
    let mix = |m: u8, g: u8| {
        (f32::from(g) + (f32::from(m) - f32::from(g)) * WASH_FIELD_MIX).round() as u8
    };
    (
        mix(mark.0, ground.0),
        mix(mark.1, ground.1),
        mix(mark.2, ground.2),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl NamedColor {
    /// Every named color, for callers that need the full palette (the
    /// native renderer derives its wash-detection table from this).
    pub const ALL: [Self; 16] = [
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::White,
        Self::BrightBlack,
        Self::BrightRed,
        Self::BrightGreen,
        Self::BrightYellow,
        Self::BrightBlue,
        Self::BrightMagenta,
        Self::BrightCyan,
        Self::BrightWhite,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "black" => Some(Self::Black),
            "red" => Some(Self::Red),
            "green" => Some(Self::Green),
            "yellow" => Some(Self::Yellow),
            "blue" => Some(Self::Blue),
            "magenta" | "purple" => Some(Self::Magenta),
            "cyan" => Some(Self::Cyan),
            "white" => Some(Self::White),
            "bright_black" | "gray" | "grey" | "bright-black" => Some(Self::BrightBlack),
            "bright_red" | "bright-red" => Some(Self::BrightRed),
            "bright_green" | "bright-green" => Some(Self::BrightGreen),
            "bright_yellow" | "bright-yellow" => Some(Self::BrightYellow),
            "bright_blue" | "bright-blue" => Some(Self::BrightBlue),
            "bright_magenta" | "bright_purple" | "bright-magenta" => Some(Self::BrightMagenta),
            "bright_cyan" | "bright-cyan" => Some(Self::BrightCyan),
            "bright_white" | "bright-white" => Some(Self::BrightWhite),
            _ => None,
        }
    }

    pub fn fg_code(self) -> u32 {
        match self {
            Self::Black => 30,
            Self::Red => 31,
            Self::Green => 32,
            Self::Yellow => 33,
            Self::Blue => 34,
            Self::Magenta => 35,
            Self::Cyan => 36,
            Self::White => 37,
            Self::BrightBlack => 90,
            Self::BrightRed => 91,
            Self::BrightGreen => 92,
            Self::BrightYellow => 93,
            Self::BrightBlue => 94,
            Self::BrightMagenta => 95,
            Self::BrightCyan => 96,
            Self::BrightWhite => 97,
        }
    }

    pub fn bg_code(self) -> u32 {
        self.fg_code() + 10
    }

    /// The quarter-strength wash tint for this color. The trigger
    /// engine bakes this exact value into washed lines as a truecolor
    /// background, and the native renderer recognizes it to paint the
    /// row's field. Both sides MUST derive it from here or detection
    /// breaks.
    pub fn wash_tint(self) -> (u8, u8, u8) {
        let (r, g, b) = self.rgb();
        (r / 4, g / 4, b / 4)
    }

    /// Canonical xterm RGB for the named color. Used to derive the
    /// truecolor full-line wash signal, so the wash tint stays stable
    /// regardless of the active theme palette.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Black => (0x00, 0x00, 0x00),
            Self::Red => (0xcd, 0x00, 0x00),
            Self::Green => (0x00, 0xcd, 0x00),
            Self::Yellow => (0xcd, 0xcd, 0x00),
            Self::Blue => (0x00, 0x00, 0xee),
            Self::Magenta => (0xcd, 0x00, 0xcd),
            Self::Cyan => (0x00, 0xcd, 0xcd),
            Self::White => (0xe5, 0xe5, 0xe5),
            Self::BrightBlack => (0x7f, 0x7f, 0x7f),
            Self::BrightRed => (0xff, 0x00, 0x00),
            Self::BrightGreen => (0x00, 0xff, 0x00),
            Self::BrightYellow => (0xff, 0xff, 0x00),
            Self::BrightBlue => (0x5c, 0x5c, 0xff),
            Self::BrightMagenta => (0xff, 0x00, 0xff),
            Self::BrightCyan => (0x00, 0xff, 0xff),
            Self::BrightWhite => (0xff, 0xff, 0xff),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{wash_field, NamedColor};

    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        ground: String,
        palette: Vec<String>,
        washes: Vec<Wash>,
    }

    #[derive(serde::Deserialize)]
    struct Wash {
        color: NamedColor,
        tint: String,
        field: String,
    }

    fn rgb(hex: &str) -> (u8, u8, u8) {
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        (byte(1), byte(3), byte(5))
    }

    #[test]
    fn wash_tints_and_fields_match_the_shared_fixture() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../../../fixtures/wash/fields.json")).unwrap();
        assert!(!fixture.cases.is_empty());
        for case in &fixture.cases {
            let colors: Vec<NamedColor> = case.washes.iter().map(|w| w.color).collect();
            assert_eq!(
                colors,
                NamedColor::ALL,
                "{} lists every color in order",
                case.name
            );
            assert_eq!(case.palette.len(), 16, "{} has 16 colors", case.name);
            let ground = rgb(&case.ground);
            for (wash, mark) in case.washes.iter().zip(&case.palette) {
                assert_eq!(
                    wash.color.wash_tint(),
                    rgb(&wash.tint),
                    "{} {:?} tint",
                    case.name,
                    wash.color
                );
                assert_eq!(
                    wash_field(rgb(mark), ground),
                    rgb(&wash.field),
                    "{} {:?} field",
                    case.name,
                    wash.color
                );
            }
        }
    }
}
