//! A text size you pick in Settings, in CSS pixels on half steps, such as
//! 13 or 13.5. The terminal size, the panel size and the command line
//! size each hold one.
//!
//! A whole size writes as an integer, `font_size = 13`, the way every
//! build before half sizes wrote it, so a profile that never picks a half
//! saves the bytes it saved before. A half writes as a float,
//! `font_size = 13.5`. A read takes either, and a hand edit off the half
//! steps, such as 13.3, reads as the nearest half. A negative number or
//! one that is no number at all reads as 0.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A text size held as a count of half pixels, so 27 is 13.5 px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct TextPx(u32);

impl TextPx {
    /// A whole size of `px` pixels.
    pub(crate) const fn whole(px: u32) -> Self {
        Self(px.saturating_mul(2))
    }

    /// `px` on the nearest half step. 0 for a negative size or one that
    /// is no number.
    pub(crate) fn from_px(px: f64) -> Self {
        if px.is_nan() || px <= 0.0 {
            return Self(0);
        }
        // The cast saturates past u32::MAX, which the clamp holds anyway.
        Self((px * 2.0).round() as u32)
    }

    /// The size in pixels.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn px(self) -> f32 {
        self.0 as f32 / 2.0
    }

    /// Whether the size is a whole number of pixels.
    pub(crate) const fn is_whole(self) -> bool {
        self.0 % 2 == 0
    }

    /// The size held to `min` to `max`.
    pub(crate) fn clamp(self, min: Self, max: Self) -> Self {
        Self(self.0.clamp(min.0, max.0))
    }
}

impl From<u32> for TextPx {
    fn from(px: u32) -> Self {
        Self::whole(px)
    }
}

impl fmt::Display for TextPx {
    /// `14` for a whole size and `13.5` for a half.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.px())
    }
}

impl Serialize for TextPx {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.is_whole() {
            serializer.serialize_u32(self.0 / 2)
        } else {
            serializer.serialize_f64(f64::from(self.0) / 2.0)
        }
    }
}

impl<'de> Deserialize<'de> for TextPx {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SizeVisitor;

        impl Visitor<'_> for SizeVisitor {
            type Value = TextPx;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a text size in pixels, such as 13 or 13.5")
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<TextPx, E> {
                Ok(TextPx::whole(u32::try_from(v).unwrap_or(u32::MAX / 2)))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<TextPx, E> {
                Ok(u64::try_from(v).map_or(TextPx(0), |v| {
                    TextPx::whole(u32::try_from(v).unwrap_or(u32::MAX / 2))
                }))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<TextPx, E> {
                Ok(TextPx::from_px(v))
            }
        }

        deserializer.deserialize_any(SizeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct Holder {
        size: TextPx,
    }

    fn read(text: &str) -> TextPx {
        toml::from_str::<Holder>(&format!("size = {text}"))
            .unwrap()
            .size
    }

    fn written(size: TextPx) -> String {
        toml::to_string(&Holder { size }).unwrap()
    }

    #[test]
    fn a_whole_size_reads_and_writes_as_the_integer_it_was() {
        assert_eq!(read("13"), TextPx::whole(13));
        assert_eq!(written(TextPx::whole(13)), "size = 13\n");
        assert_eq!(read("0"), TextPx::whole(0));
        assert_eq!(written(TextPx::whole(0)), "size = 0\n");
    }

    #[test]
    fn a_half_size_reads_and_writes_as_a_float() {
        assert_eq!(read("13.5"), TextPx::from_px(13.5));
        assert_eq!(written(TextPx::from_px(13.5)), "size = 13.5\n");
        assert_eq!(read("13.0"), TextPx::whole(13));
        assert_eq!(written(read("13.0")), "size = 13\n");
    }

    #[test]
    fn an_odd_fraction_reads_as_the_nearest_half() {
        assert_eq!(read("13.3"), TextPx::from_px(13.5));
        assert_eq!(read("13.2"), TextPx::whole(13));
        assert_eq!(read("13.8"), TextPx::whole(14));
        assert_eq!(read("13.75"), TextPx::whole(14));
        assert_eq!(read("13.25"), TextPx::from_px(13.5));
    }

    #[test]
    fn a_negative_or_broken_size_reads_as_zero() {
        assert_eq!(read("-4"), TextPx::whole(0));
        assert_eq!(read("-4.5"), TextPx::whole(0));
        assert_eq!(read("nan"), TextPx::whole(0));
        assert!(toml::from_str::<Holder>("size = \"13\"").is_err());
    }

    #[test]
    fn json_carries_halves_and_wholes() {
        let size: TextPx = serde_json::from_str("13.5").unwrap();
        assert_eq!(size, TextPx::from_px(13.5));
        assert_eq!(serde_json::to_string(&size).unwrap(), "13.5");
        let size: TextPx = serde_json::from_str("16").unwrap();
        assert_eq!(serde_json::to_string(&size).unwrap(), "16");
    }

    #[test]
    fn display_and_clamp() {
        assert_eq!(TextPx::from_px(13.5).to_string(), "13.5");
        assert_eq!(TextPx::whole(14).to_string(), "14");
        let (lo, hi) = (TextPx::whole(6), TextPx::whole(64));
        assert_eq!(TextPx::from_px(5.5).clamp(lo, hi), lo);
        assert_eq!(TextPx::from_px(64.5).clamp(lo, hi), hi);
        assert_eq!(TextPx::from_px(6.5).clamp(lo, hi), TextPx::from_px(6.5));
    }
}
