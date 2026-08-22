//! Named ANSI 16 colors. Phase 3 keeps the highlight palette to the named
//! set; xterm 256 and 24 bit truecolor land in a later phase.

use serde::{Deserialize, Serialize};

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

    /// Canonical xterm RGB for the named color. Used to derive the
    /// truecolor full-line wash and the renderer's accent bar, so the
    /// wash tint stays stable regardless of the active theme palette.
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
