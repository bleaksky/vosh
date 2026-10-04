//! The `MUSHclient` importer, for world files and plugin packages.

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{Trigger, TriggerAction};

use super::{attr, ImportReport};

// World files have a flat layout: `<muclient><world>...</world>` (or
// `<plugin>...</plugin>` for plugin packages) containing direct
// `<aliases>`, `<triggers>`, `<timers>`, `<variables>` blocks. Each
// `<alias>` and `<trigger>` is a self-closing element whose entire
// payload sits in attributes.
//
// Example:
//   <alias name="g" match="g" enabled="y" send="get $1.gold"
//          regexp="n" sequence="100"/>
//   <trigger name="combat_dmg" enabled="y" match="^You hit"
//            send="" regexp="y" sequence="100" group="combat"
//            colour="12"/>
//
// MUSHclient stores keyboard macros in the global preferences, NOT
// in the world XML, so we never see them here.

pub(super) fn parse_mushclient(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e) | Event::Start(e)) => match e.name().as_ref() {
                b"alias" => {
                    if let Some(alias) = mushclient_alias_from(&e) {
                        report.aliases.push(alias);
                    } else {
                        report
                            .unparsed
                            .push(format!("alias missing match/send: {}", debug_tag(&e)));
                    }
                }
                b"trigger" => {
                    if let Some(trigger) = mushclient_trigger_from(&e, &mut report) {
                        report.triggers.push(trigger);
                    } else {
                        report
                            .unparsed
                            .push(format!("trigger missing match: {}", debug_tag(&e)));
                    }
                }
                b"variable" => {
                    let name = attr(&e, b"name").unwrap_or_default();
                    let value = attr(&e, b"value").unwrap_or_default();
                    if !name.is_empty() {
                        report.vars.push((name, value));
                    }
                }
                b"timer" => report
                    .unsupported
                    .push(("timer".into(), attr(&e, b"name").unwrap_or_default())),
                b"plugin_script" | b"script" => {
                    report
                        .unsupported
                        .push(("script".into(), attr(&e, b"name").unwrap_or_default()));
                }
                _ => {}
            },
            Ok(_) => {}
            Err(e) => {
                report.unparsed.push(format!("xml error: {e}"));
                break;
            }
        }
    }
    report
}

fn mushclient_alias_from(e: &BytesStart) -> Option<Alias> {
    let pattern = attr(e, b"match")?;
    let send = attr(e, b"send")?;
    if pattern.is_empty() {
        return None;
    }
    let name = attr(e, b"name").unwrap_or_else(|| pattern.clone());
    let enabled =
        attr(e, b"enabled").map_or(true, |v| matches!(v.as_str(), "y" | "yes" | "true" | "1"));
    Some(Alias {
        name,
        expansion: send,
        enabled,
        group: None,
        script: None,
    })
}

fn mushclient_trigger_from(e: &BytesStart, report: &mut ImportReport) -> Option<Trigger> {
    let pattern = attr(e, b"match")?;
    if pattern.is_empty() {
        return None;
    }
    let name = attr(e, b"name")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("imported_{}", report.triggers.len() + 1));
    let send = attr(e, b"send").unwrap_or_default();
    let enabled =
        attr(e, b"enabled").map_or(true, |v| matches!(v.as_str(), "y" | "yes" | "true" | "1"));
    let sequence = attr(e, b"sequence")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(100);
    // MUSHclient triggers can carry a `colour` attribute (highlight)
    // and/or a `send` body. We model the send half; the colour half
    // is dropped since the integer palette index does not round-trip
    // to our ANSI/hex highlight styles cleanly. Note that as an
    // unsupported feature so the user knows.
    if attr(e, b"colour").is_some_and(|s| !s.is_empty()) {
        report.unsupported.push((
            "trigger-colour".into(),
            format!("{name} (palette index dropped)"),
        ));
    }
    let mut actions: Vec<TriggerAction> = Vec::new();
    if !send.is_empty() {
        actions.push(TriggerAction::Send { template: send });
    }
    Some(Trigger {
        name,
        patterns: vec![vosh_automation::trigger::TriggerPattern::regex(pattern)],
        priority: sequence,
        enabled,
        group: None,
        actions,
        preset: None,
        target: vosh_automation::trigger::TriggerTarget::Line,
        alert: None,
    })
}

fn debug_tag(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mushclient_alias_and_trigger() {
        let xml = r#"<muclient><world>
            <aliases>
                <alias name="g" match="g" enabled="y" send="get gold"/>
            </aliases>
            <triggers>
                <trigger name="hit" enabled="y" match="^You hit"
                         send="kick" regexp="y" sequence="50"/>
            </triggers>
        </world></muclient>"#;
        let r = parse_mushclient(xml);
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].name, "g");
        assert_eq!(r.aliases[0].expansion, "get gold");
        assert_eq!(r.triggers.len(), 1);
        assert_eq!(r.triggers[0].first_pattern(), "^You hit");
        assert_eq!(r.triggers[0].priority, 50);
        assert_eq!(r.triggers[0].actions.len(), 1);
    }
}
