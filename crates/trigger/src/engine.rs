//! Match the trigger store against a single line and produce the resulting
//! display text plus any side effects.

use regex::Regex;
use vosh_ansi::plain_text;

use crate::action::{HighlightStyle, TriggerAction};
use crate::store::TriggerStore;

/// What the engine produced for one line of MUD output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineResult {
    /// Text to display. `None` means the line was gagged.
    pub display: Option<String>,
    /// Commands to send to the server, in priority order.
    pub sends: Vec<String>,
    /// Pane names to route the line to. Phase 5 wires the panes; until then
    /// the session loop logs these.
    pub routes: Vec<String>,
}

/// Run the trigger store against a single line of MUD output.
///
/// `original` is the raw bytes of the line as the server sent it, including
/// any embedded ANSI escapes. The trigger engine matches against the plain
/// text (escapes stripped) and rebuilds the display string from there.
pub fn process(store: &TriggerStore, original: &[u8]) -> LineResult {
    let plain = plain_text(original);

    if store.is_empty() {
        return LineResult {
            display: Some(bytes_to_string_lossy(original)),
            sends: Vec::new(),
            routes: Vec::new(),
        };
    }

    let mut gagged = false;
    let mut text = plain.clone();
    let mut highlights: Vec<(Regex, HighlightStyle)> = Vec::new();
    let mut sends = Vec::new();
    let mut routes = Vec::new();
    let mut any_match = false;

    for compiled in store.iter_compiled() {
        if !compiled.trigger.enabled {
            continue;
        }
        // A trigger can hold many patterns (Mudlet-style). Each enabled
        // pattern is its own regex; we walk them all and run the
        // shared actions for every one that matches the line. This
        // keeps capture groups specific to the matching pattern, so a
        // Send template with `$1` references the correct capture
        // regardless of which sibling pattern fired.
        for regex in &compiled.regexes {
            let mut iter = regex.captures_iter(&plain).peekable();
            if iter.peek().is_none() {
                continue;
            }
            any_match = true;

            for action in &compiled.trigger.actions {
                match action {
                    TriggerAction::Gag => {
                        gagged = true;
                    }
                    TriggerAction::Replace { template } => {
                        text = regex.replace_all(&text, template.as_str()).into_owned();
                    }
                    TriggerAction::Highlight { style } => {
                        if !style.is_empty() {
                            highlights.push((regex.clone(), style.clone()));
                        }
                    }
                    TriggerAction::Send { template } => {
                        for caps in regex.captures_iter(&plain) {
                            let mut buf = String::new();
                            caps.expand(template, &mut buf);
                            sends.push(buf);
                        }
                    }
                    TriggerAction::Route { pane } => {
                        routes.push(pane.clone());
                    }
                }
            }
        }
    }

    if gagged {
        return LineResult {
            display: None,
            sends,
            routes,
        };
    }

    let display = if any_match {
        // Apply highlights last on the (possibly replaced) text so colors
        // wrap whatever the user ends up seeing.
        for (regex, style) in highlights {
            text = wrap_matches(&text, &regex, &style);
        }
        Some(text)
    } else {
        Some(bytes_to_string_lossy(original))
    };

    LineResult {
        display,
        sends,
        routes,
    }
}

fn wrap_matches(text: &str, regex: &Regex, style: &HighlightStyle) -> String {
    let open = style.sgr_open();
    let close = HighlightStyle::sgr_reset();
    let mut out = String::with_capacity(text.len() + open.len() * 2);
    let mut last = 0;
    for mat in regex.find_iter(text) {
        if mat.start() > last {
            out.push_str(&text[last..mat.start()]);
        }
        out.push_str(&open);
        out.push_str(mat.as_str());
        out.push_str(close);
        last = mat.end();
    }
    if last < text.len() {
        out.push_str(&text[last..]);
    }
    out
}

fn bytes_to_string_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::HighlightStyle;
    use crate::color::NamedColor;
    use crate::store::Trigger;

    fn store(triggers: Vec<Trigger>) -> TriggerStore {
        let mut s = TriggerStore::new();
        for t in triggers {
            s.set(t).unwrap();
        }
        s
    }

    fn highlight(name: &str, pattern: &str, fg: NamedColor) -> Trigger {
        Trigger {
            name: name.to_string(),
            patterns: single_pattern(pattern),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(fg),
                    ..Default::default()
                },
            }],
            preset: None,
        }
    }

    fn single_pattern(p: &str) -> Vec<crate::store::TriggerPattern> {
        vec![crate::store::TriggerPattern {
            pattern: p.to_string(),
            enabled: true,
        }]
    }

    #[test]
    fn no_triggers_returns_original() {
        let s = TriggerStore::new();
        let r = process(&s, b"plain text");
        assert_eq!(r.display.as_deref(), Some("plain text"));
        assert!(r.sends.is_empty());
    }

    #[test]
    fn no_match_returns_original() {
        let s = store(vec![highlight("h", "goblin", NamedColor::Cyan)]);
        let r = process(&s, b"a peaceful meadow");
        assert_eq!(r.display.as_deref(), Some("a peaceful meadow"));
    }

    #[test]
    fn highlight_wraps_matched_substring() {
        let s = store(vec![highlight("tells", r"\w+ tells you", NamedColor::Cyan)]);
        let r = process(&s, b"Bob tells you 'hi'");
        let text = r.display.unwrap();
        assert!(text.contains("\x1b[36m"));
        assert!(text.contains("Bob tells you"));
        assert!(text.contains("\x1b[0m"));
    }

    #[test]
    fn gag_drops_line() {
        let s = store(vec![Trigger {
            name: "spam".into(),
            patterns: single_pattern("tingle"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Gag],
            preset: None,
        }]);
        let r = process(&s, b"You feel a tingle.");
        assert!(r.display.is_none());
    }

    #[test]
    fn replace_substitutes_with_capture() {
        let s = store(vec![Trigger {
            name: "rename".into(),
            patterns: single_pattern(r"goblin"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Replace {
                template: "wolf".into(),
            }],
            preset: None,
        }]);
        let r = process(&s, b"You see a goblin.");
        assert_eq!(r.display.as_deref(), Some("You see a wolf."));
    }

    #[test]
    fn replace_uses_named_capture() {
        let s = store(vec![Trigger {
            name: "polite".into(),
            patterns: single_pattern(r"(?<who>\w+) yells"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Replace {
                template: "$who calmly says".into(),
            }],
            preset: None,
        }]);
        let r = process(&s, b"Bob yells");
        assert_eq!(r.display.as_deref(), Some("Bob calmly says"));
    }

    #[test]
    fn send_substitutes_capture() {
        let s = store(vec![Trigger {
            name: "loot".into(),
            patterns: single_pattern(r"The (\w+) is DEAD"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Send {
                template: "loot $1".into(),
            }],
            preset: None,
        }]);
        let r = process(&s, b"The goblin is DEAD!");
        assert_eq!(r.sends, vec!["loot goblin".to_string()]);
    }

    #[test]
    fn route_appends_pane_name() {
        let s = store(vec![Trigger {
            name: "tells".into(),
            patterns: single_pattern(r"tells you"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Route {
                pane: "chat".into(),
            }],
            preset: None,
        }]);
        let r = process(&s, b"Bob tells you 'hi'");
        assert_eq!(r.routes, vec!["chat".to_string()]);
    }

    #[test]
    fn priority_order_is_high_to_low() {
        let mut s = TriggerStore::new();
        s.set(Trigger {
            name: "low".into(),
            patterns: single_pattern("x"),
            priority: -10,
            enabled: true,
            actions: vec![TriggerAction::Replace {
                template: "L".into(),
            }],
            preset: None,
        })
        .unwrap();
        s.set(Trigger {
            name: "high".into(),
            patterns: single_pattern("x"),
            priority: 100,
            enabled: true,
            actions: vec![TriggerAction::Replace {
                template: "H".into(),
            }],
            preset: None,
        })
        .unwrap();
        // High runs first on plain text "x". After high replaces to "H",
        // low matches against original plain text "x" (no match in
        // resulting "H"), so its replace also runs on the original "x"
        // pattern against current text "H" which finds nothing. Net result
        // is "H".
        let r = process(&s, b"x");
        assert_eq!(r.display.as_deref(), Some("H"));
    }

    #[test]
    fn disabled_trigger_does_not_fire() {
        let mut s = TriggerStore::new();
        let mut t = highlight("tells", r"tells you", NamedColor::Cyan);
        t.enabled = false;
        s.set(t).unwrap();
        let r = process(&s, b"Bob tells you 'hi'");
        let text = r.display.unwrap();
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn ansi_in_input_is_stripped_for_match() {
        let s = store(vec![highlight("tells", r"Bob tells you", NamedColor::Cyan)]);
        // Server sends gray text. Trigger should still match.
        let r = process(&s, b"\x1b[37mBob tells you 'hi'\x1b[0m");
        let text = r.display.unwrap();
        assert!(text.contains("\x1b[36m"));
        assert!(text.contains("Bob tells you"));
    }

    #[test]
    fn invalid_regex_rejected_at_set() {
        let mut s = TriggerStore::new();
        let bad = Trigger {
            name: "bad".into(),
            patterns: single_pattern("[unclosed"),
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Gag],
            preset: None,
        };
        assert!(s.set(bad).is_err());
    }

    #[test]
    fn multi_pattern_matches_either_row() {
        let s = store(vec![Trigger {
            name: "mobs".into(),
            patterns: vec![
                crate::store::TriggerPattern {
                    pattern: "goblin".into(),
                    enabled: true,
                },
                crate::store::TriggerPattern {
                    pattern: "orc".into(),
                    enabled: true,
                },
            ],
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(NamedColor::Red),
                    ..Default::default()
                },
            }],
            preset: None,
        }]);
        let r1 = process(&s, b"You see a goblin.");
        assert!(r1.display.unwrap().contains("\x1b[31m"));
        let r2 = process(&s, b"An orc charges.");
        assert!(r2.display.unwrap().contains("\x1b[31m"));
    }

    #[test]
    fn disabled_pattern_row_is_skipped() {
        let s = store(vec![Trigger {
            name: "mobs".into(),
            patterns: vec![
                crate::store::TriggerPattern {
                    pattern: "goblin".into(),
                    enabled: true,
                },
                crate::store::TriggerPattern {
                    pattern: "orc".into(),
                    enabled: false,
                },
            ],
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Gag],
            preset: None,
        }]);
        // goblin pattern (enabled) gags.
        assert!(process(&s, b"You see a goblin.").display.is_none());
        // orc pattern (disabled) does not fire.
        assert_eq!(
            process(&s, b"An orc charges.").display.as_deref(),
            Some("An orc charges.")
        );
    }

    #[test]
    fn import_export_round_trip() {
        let mut s = TriggerStore::new();
        s.set(highlight("tells", r"tells you", NamedColor::Cyan))
            .unwrap();
        s.set(Trigger {
            name: "spam".into(),
            patterns: single_pattern("tingle"),
            priority: 50,
            enabled: true,
            actions: vec![TriggerAction::Gag],
            preset: None,
        })
        .unwrap();
        let json = s.export_json().unwrap();

        let mut s2 = TriggerStore::new();
        let count = s2.import_json(&json).unwrap();
        assert_eq!(count, 2);
        assert_eq!(s.list(), s2.list());
    }
}
