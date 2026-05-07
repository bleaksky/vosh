//! Color model. Covers the three layers a server might mix in one stream.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    #[default]
    Default,
    /// ANSI 8 color (0..=7) and the bright variants (8..=15).
    Indexed16(u8),
    /// xterm 256 color palette index.
    Indexed256(u8),
    /// 24 bit truecolor.
    Rgb { r: u8, g: u8, b: u8 },
}
