//! What an alert does: the banner, the sound and the attention call, and
//! when it rings. A trigger keeps one in an `alert` table of its own
//! beside its actions, an alert preset keeps one in the profile's
//! `[alerts]` table, and `mud.alert` builds one, so the three share this
//! shape.
//!
//! On disk an alert reads like this, every key optional:
//!
//! ```toml
//! [triggers.alert]
//! banner = true
//! sound = "chime"
//! attention = "once"
//! background = true
//! words = true
//! ```
//!
//! A table a build does not know is skipped, so a build up to 0.8.1
//! still reads a trigger with an alert and keeps its actions. A
//! sound or an attention this build does not know reads as none, so one
//! value never fails the whole file.

use serde::{Deserialize, Deserializer, Serialize};

/// What an alert does when it rings. Each part turns on and off on its
/// own, so an alert with none of them on does nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertParts {
    /// Post a system banner, a toast on Windows.
    #[serde(default)]
    pub banner: bool,
    /// The tone the page plays, such as `chime`, `bell`, `knock` or
    /// `low`. None plays nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    /// Bounce the Dock icon, flash the taskbar on Windows, or set the
    /// urgency hint on Linux. None asks for nothing.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "attention_or_none"
    )]
    pub attention: Option<Attention>,
    /// Ring only while you are not looking at the session, Only while you
    /// are not looking at its session in Settings. On at first.
    #[serde(default = "on")]
    pub background: bool,
    /// The banner shows the words, Title and words in Settings, and not
    /// the title alone. Off at first, since a banner lands on shared and
    /// locked screens.
    #[serde(default)]
    pub words: bool,
}

impl Default for AlertParts {
    /// Nothing on, ringing only while you are not looking.
    fn default() -> Self {
        Self {
            banner: false,
            sound: None,
            attention: None,
            background: true,
            words: false,
        }
    }
}

impl AlertParts {
    /// True when no part would do anything, so the alert never rings.
    pub fn is_silent(&self) -> bool {
        !self.banner && self.sound.is_none() && self.attention.is_none()
    }
}

/// How long the Dock bounces or the taskbar flashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    /// Once. Informational on macOS, a single bounce.
    Once,
    /// Until you come back to Vosh. Critical on macOS, which bounces
    /// until Vosh comes to the front, and a flash until then on Windows.
    Until,
}

fn on() -> bool {
    true
}

/// An attention value, or None for one this build does not know.
fn attention_or_none<'de, D>(deserializer: D) -> Result<Option<Attention>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Known(Attention),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Option::<Raw>::deserialize(deserializer)? {
        Some(Raw::Known(attention)) => Some(attention),
        Some(Raw::Other(_)) | None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{AlertParts, Attention};

    #[test]
    fn a_table_with_no_keys_is_silent_and_rings_only_in_the_background() {
        let parts: AlertParts = serde_json::from_str("{}").unwrap();
        assert_eq!(parts, AlertParts::default());
        assert!(parts.is_silent());
        assert!(parts.background);
    }

    #[test]
    fn an_attention_this_build_does_not_know_reads_as_none() {
        let parts: AlertParts =
            serde_json::from_str(r#"{"banner":true,"attention":"forever"}"#).unwrap();
        assert!(parts.banner);
        assert_eq!(parts.attention, None);
        let parts: AlertParts = serde_json::from_str(r#"{"attention":"until"}"#).unwrap();
        assert_eq!(parts.attention, Some(Attention::Until));
    }

    #[test]
    fn the_file_shape_round_trips_and_leaves_out_what_is_off() {
        let parts = AlertParts {
            banner: true,
            sound: Some("chime".into()),
            attention: Some(Attention::Once),
            background: true,
            words: true,
        };
        let text = serde_json::to_string(&parts).unwrap();
        assert_eq!(
            text,
            r#"{"banner":true,"sound":"chime","attention":"once","background":true,"words":true}"#
        );
        assert_eq!(serde_json::from_str::<AlertParts>(&text).unwrap(), parts);
        let quiet = serde_json::to_string(&AlertParts::default()).unwrap();
        assert_eq!(quiet, r#"{"banner":false,"background":true,"words":false}"#);
    }
}
