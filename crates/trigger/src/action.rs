//! Trigger actions.

use serde::{Deserialize, Serialize};

use crate::color::NamedColor;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<NamedColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<NamedColor>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inverse: bool,
}

impl HighlightStyle {
    /// Build the SGR open sequence (e.g. "\x1b[1;36m"). Empty string when no
    /// codes apply.
    pub fn sgr_open(&self) -> String {
        let mut codes: Vec<u32> = Vec::new();
        if self.bold {
            codes.push(1);
        }
        if self.underline {
            codes.push(4);
        }
        if self.inverse {
            codes.push(7);
        }
        if let Some(c) = self.fg {
            codes.push(c.fg_code());
        }
        if let Some(c) = self.bg {
            codes.push(c.bg_code());
        }
        if codes.is_empty() {
            return String::new();
        }
        let parts: Vec<String> = codes.iter().map(u32::to_string).collect();
        format!("\x1b[{}m", parts.join(";"))
    }

    /// Reset SGR sequence. Always the same string.
    pub fn sgr_reset() -> &'static str {
        "\x1b[0m"
    }

    pub fn is_empty(&self) -> bool {
        self.fg.is_none() && self.bg.is_none() && !self.bold && !self.underline && !self.inverse
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerAction {
    /// Wrap matched text in SGR open and SGR reset.
    Highlight { style: HighlightStyle },
    /// Drop the line entirely.
    Gag,
    /// Replace the matched portion with a template that supports `$0`, `$1`,
    /// and `${name}` capture references.
    Replace { template: String },
    /// Send a command back to the server. Template supports captures.
    Send { template: String },
    /// Route the line to a named pane. Phase 5 wires panes; until then the
    /// session loop logs the route.
    Route { pane: String },
    /// Evaluate a Lua body in the session's `ScriptEngine` with regex
    /// captures bound as a local `captures` table (`captures[1]`,
    /// `[2]`, … plus named captures by key). The body has access to
    /// the same sandboxed `mud.*` API as standalone Lua scripts —
    /// `mud.send`, `mud.echo`, `mud.log`, etc.
    Script { body: String },
}
