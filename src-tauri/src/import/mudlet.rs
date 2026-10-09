//! The Mudlet importer, for package exports.

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{Trigger, TriggerAction};

use super::{attr, ImportReport};
use crate::profile::live::Macro;

// Mudlet packages are hierarchical: each item is a full element with
// child tags carrying the payload (not attributes). Items can also
// be folders containing nested items. The shape is roughly:
//
//   <MudletPackage version="1.001">
//     <TriggerPackage>
//       <TriggerGroup ...>
//         <Trigger isActive="yes" isFolder="no" ...>
//           <name>...</name>
//           <script>send("kick")</script>
//           <regexCodeList>
//             <string>^You hit</string>
//           </regexCodeList>
//         </Trigger>
//       </TriggerGroup>
//     </TriggerPackage>
//     <AliasPackage>...</AliasPackage>
//     <KeyPackage>...</KeyPackage>
//   </MudletPackage>
//
// Mudlet aliases/triggers carry Lua `script` bodies. We import
// vosh-friendly `send("...")` and `send [[...]]` shells into the
// alias/trigger's send-action; arbitrary Lua goes into unsupported.
// Keys map onto vosh Macros via the Qt key code -> canonical
// string conversion.

// Stack of in-progress items + the text-bearing child currently being
// collected. Mudlet items are hierarchical (Trigger, Alias, Key contain
// <name>, <script>, <regex>, etc.), so we accumulate text between
// matching Start/End pairs.
enum MudletStackItem {
    Trigger(MudletItem),
    Alias(MudletItem),
    Key(MudletItem),
}

pub(super) fn parse_mudlet(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<MudletStackItem> = Vec::new();
    let mut text_target: Option<String> = None;
    let mut current_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "Trigger" => stack.push(MudletStackItem::Trigger(MudletItem::from_attrs(&e))),
                    "Alias" => stack.push(MudletStackItem::Alias(MudletItem::from_attrs(&e))),
                    "Key" => stack.push(MudletStackItem::Key(MudletItem::from_attrs(&e))),
                    "name" | "script" | "regex" | "command" | "keyCode" | "keyModifier"
                    | "pattern" | "string" => {
                        text_target = Some(name);
                        current_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // Self-closing items have no body so we cannot
                // capture text fields from them. Pattern-only tags
                // like <regex/> still register as an empty target.
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "Trigger" => {
                        commit_mudlet_trigger(MudletItem::from_attrs(&e), &mut report);
                    }
                    "Alias" => {
                        commit_mudlet_alias(MudletItem::from_attrs(&e), &mut report);
                    }
                    "Key" => {
                        commit_mudlet_key(MudletItem::from_attrs(&e), &mut report);
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if text_target.is_some() {
                    current_text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "name" | "script" | "regex" | "command" | "keyCode" | "keyModifier"
                    | "pattern" | "string" => {
                        if let Some(target) = text_target.take() {
                            if let Some(top) = stack.last_mut() {
                                let item = match top {
                                    MudletStackItem::Trigger(i)
                                    | MudletStackItem::Alias(i)
                                    | MudletStackItem::Key(i) => i,
                                };
                                item.put(&target, current_text.clone());
                            }
                            current_text.clear();
                        }
                    }
                    "Trigger" => {
                        if let Some(MudletStackItem::Trigger(item)) = stack.pop() {
                            commit_mudlet_trigger(item, &mut report);
                        }
                    }
                    "Alias" => {
                        if let Some(MudletStackItem::Alias(item)) = stack.pop() {
                            commit_mudlet_alias(item, &mut report);
                        }
                    }
                    "Key" => {
                        if let Some(MudletStackItem::Key(item)) = stack.pop() {
                            commit_mudlet_key(item, &mut report);
                        }
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
            Err(e) => {
                report.unparsed.push(format!("xml error: {e}"));
                break;
            }
        }
    }
    report
}

#[derive(Default, Debug)]
struct MudletItem {
    is_active: bool,
    name: String,
    /// Lua body. Often `send("blah")`; we extract the literal arg
    /// when it matches a known shape.
    script: String,
    /// Trigger regex / alias regex pattern.
    pattern: String,
    /// Key item's Qt key code (integer string).
    key_code: String,
    /// Key item's Qt modifier mask (integer string).
    key_modifier: String,
}

impl MudletItem {
    fn from_attrs(e: &BytesStart) -> Self {
        Self {
            is_active: attr(e, b"isActive")
                .map_or(true, |v| matches!(v.as_str(), "yes" | "true" | "1")),
            ..Self::default()
        }
    }

    fn put(&mut self, field: &str, value: String) {
        match field {
            "name" => self.name = value,
            // Mudlet triggers/aliases use "script" for the Lua body;
            // Mudlet keys use "command" for the same purpose. Treat
            // them as one slot.
            "script" | "command" => self.script = value,
            "regex" => self.pattern = value,
            "pattern" | "string"
                // Trigger has <regexCodeList><string>regex</string></regexCodeList>
                // Take the first non-empty pattern.
                if self.pattern.is_empty() => {
                    self.pattern = value;
                }
            "keyCode" => self.key_code = value,
            "keyModifier" => self.key_modifier = value,
            _ => {}
        }
    }
}

fn commit_mudlet_trigger(item: MudletItem, report: &mut ImportReport) {
    if item.pattern.is_empty() {
        return;
    }
    let name = if item.name.is_empty() {
        format!("imported_{}", report.triggers.len() + 1)
    } else {
        item.name.clone()
    };
    let mut actions: Vec<TriggerAction> = Vec::new();
    if let Some(send) = extract_send_command(&item.script) {
        actions.push(TriggerAction::Send { template: send });
    } else if !item.script.trim().is_empty() {
        report
            .unsupported
            .push(("trigger-lua-script".into(), item.name.clone()));
    }
    report.triggers.push(Trigger {
        name,
        patterns: vec![vosh_automation::trigger::TriggerPattern::regex(
            item.pattern,
        )],
        priority: 100,
        enabled: item.is_active,
        actions,
        preset: None,
        group: None,
        target: vosh_automation::trigger::TriggerTarget::Line,
        alert: None,
    });
}

fn commit_mudlet_alias(item: MudletItem, report: &mut ImportReport) {
    if item.pattern.is_empty() {
        return;
    }
    let name = if item.name.is_empty() {
        item.pattern.clone()
    } else {
        item.name.clone()
    };
    let expansion = extract_send_command(&item.script).unwrap_or_else(|| {
        if !item.script.trim().is_empty() {
            report
                .unsupported
                .push(("alias-lua-script".into(), name.clone()));
        }
        String::new()
    });
    if expansion.is_empty() {
        return;
    }
    report.aliases.push(Alias {
        name,
        expansion,
        enabled: item.is_active,
        group: None,
        script: None,
    });
}

fn commit_mudlet_key(item: MudletItem, report: &mut ImportReport) {
    let Some(canonical) = qt_key_to_canonical(&item.key_code, &item.key_modifier) else {
        report.unsupported.push((
            "key-binding".into(),
            format!("{} (Qt key {})", item.name, item.key_code),
        ));
        return;
    };
    let command = extract_send_command(&item.script).unwrap_or_else(|| item.script.clone());
    if command.trim().is_empty() {
        return;
    }
    report.macros.push(Macro {
        key: canonical,
        command,
        group: None,
        enabled: true,
        preset: None,
    });
}

/// Pull a `send("blah")`, `send [[blah]]`, or `send 'blah'` out of
/// a Mudlet Lua script body. Returns None on anything more complex.
fn extract_send_command(script: &str) -> Option<String> {
    let trimmed = script.trim();
    if trimmed.is_empty() {
        return None;
    }
    // send("...")  or  send('...')
    if let Some(rest) = trimmed.strip_prefix("send(") {
        let inner = rest.trim_end_matches(')').trim();
        if let Some(s) = strip_quoted(inner) {
            return Some(s);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("send ") {
        if let Some(s) = strip_quoted(rest.trim()) {
            return Some(s);
        }
        // send [[blah]]
        if let Some(inner) = rest
            .trim()
            .strip_prefix("[[")
            .and_then(|s| s.strip_suffix("]]"))
        {
            return Some(inner.to_string());
        }
    }
    None
}

fn strip_quoted(s: &str) -> Option<String> {
    if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
        && s.len() >= 2
    {
        return Some(s[1..s.len() - 1].to_string());
    }
    None
}

/// Map a Qt key code (decimal or 0xHEX string) + modifier mask to
/// vosh's canonical key string. Covers the keys vosh actually
/// supports; everything else returns None and gets flagged.
fn qt_key_to_canonical(code: &str, modifier: &str) -> Option<String> {
    let code_n = parse_int(code)?;
    let mod_n = parse_int(modifier).unwrap_or(0);
    let base = qt_key_base(code_n)?;
    let mut parts: Vec<&str> = Vec::new();
    // Qt::KeyboardModifier: Shift=0x02000000, Control=0x04000000,
    // Alt=0x08000000, Meta=0x10000000, Keypad=0x20000000.
    if mod_n & 0x0400_0000 != 0 {
        parts.push("Ctrl");
    }
    if mod_n & 0x0800_0000 != 0 {
        parts.push("Alt");
    }
    if mod_n & 0x0200_0000 != 0 {
        parts.push("Shift");
    }
    if mod_n & 0x1000_0000 != 0 {
        parts.push("Meta");
    }
    let mut s = parts.join("+");
    if !s.is_empty() {
        s.push('+');
    }
    s.push_str(&base);
    Some(s)
}

fn parse_int(s: &str) -> Option<i64> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<i64>().ok()
    }
}

fn qt_key_base(code: i64) -> Option<String> {
    // Qt::Key constants (https://doc.qt.io/qt-6/qt.html#Key-enum)
    Some(match code {
        // Letters: Qt::Key_A=0x41 ... Qt::Key_Z=0x5A (same as ASCII)
        c if (0x41..=0x5A).contains(&c) => ((c as u8) as char).to_string(),
        // Digits
        c if (0x30..=0x39).contains(&c) => ((c as u8) as char).to_string(),
        // Function keys F1..F35 = 0x01000030 ..
        c if (0x0100_0030..=0x0100_0052).contains(&c) => {
            let n = c - 0x0100_0030 + 1;
            format!("F{n}")
        }
        0x0100_0000 => "Escape".into(),
        0x0100_0001 => "Tab".into(),
        0x0100_0003 => "Backspace".into(),
        0x0100_0004 | 0x0100_0005 => "Enter".into(),
        0x0100_0006 => "Insert".into(),
        0x0100_0007 => "Delete".into(),
        0x0100_0010 => "Home".into(),
        0x0100_0011 => "End".into(),
        0x0100_0012 => "ArrowLeft".into(),
        0x0100_0013 => "ArrowUp".into(),
        0x0100_0014 => "ArrowRight".into(),
        0x0100_0015 => "ArrowDown".into(),
        0x0100_0016 => "PageUp".into(),
        0x0100_0017 => "PageDown".into(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mudlet_alias_with_send_call() {
        let xml = r#"<MudletPackage version="1.001">
            <AliasPackage>
                <Alias isActive="yes" isFolder="no">
                    <name>g</name>
                    <script>send("get $1.gold")</script>
                    <regex>^g (.*)$</regex>
                </Alias>
            </AliasPackage>
        </MudletPackage>"#;
        let r = parse_mudlet(xml);
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].name, "g");
        assert_eq!(r.aliases[0].expansion, "get $1.gold");
        assert!(r.aliases[0].enabled);
    }

    #[test]
    fn mudlet_trigger_with_pattern_list() {
        let xml = r#"<MudletPackage>
            <TriggerPackage>
                <Trigger isActive="yes">
                    <name>combat</name>
                    <script>send("kick")</script>
                    <regexCodeList>
                        <string>^You hit</string>
                    </regexCodeList>
                </Trigger>
            </TriggerPackage>
        </MudletPackage>"#;
        let r = parse_mudlet(xml);
        assert_eq!(r.triggers.len(), 1);
        assert_eq!(r.triggers[0].first_pattern(), "^You hit");
        assert_eq!(r.triggers[0].actions.len(), 1);
    }

    /// A trigger named for a preset trigger joins the clash list and
    /// never replaces the preset's, its preset on or off, since the
    /// next install would put the preset's back.
    #[test]
    fn a_mudlet_trigger_named_for_a_preset_trigger_clashes() {
        use crate::import::vosh::{Clash, ClashKind};
        use crate::import::{merge_triggers, TriggersMerged};
        use vosh_automation::trigger::{TriggerAction, TriggerStore};
        let xml = r#"<MudletPackage>
            <TriggerPackage>
                <Trigger isActive="yes">
                    <name>disarm.secondary</name>
                    <script>send("get 1.;wield 1.")</script>
                    <regexCodeList>
                        <string>disarms you and sends your weapon flying</string>
                    </regexCodeList>
                </Trigger>
                <Trigger isActive="yes">
                    <name>combat</name>
                    <script>send("kick")</script>
                    <regexCodeList>
                        <string>^You hit</string>
                    </regexCodeList>
                </Trigger>
            </TriggerPackage>
        </MudletPackage>"#;
        let r = parse_mudlet(xml);
        let clash = TriggersMerged {
            rejected: Vec::new(),
            clashes: vec![Clash {
                kind: ClashKind::Trigger,
                name: "disarm.secondary".into(),
            }],
        };
        // Its preset off, so only the library knows the name.
        let mut store = TriggerStore::new();
        let library = ["disarm.secondary".to_string()];
        assert_eq!(merge_triggers(&mut store, &r.triggers, &library), clash);
        assert!(store.get("disarm.secondary").is_none());
        assert!(store.get("combat").is_some());

        // Its preset on, so the store holds the preset's, which stays.
        let mut store = TriggerStore::new();
        let mut preset = vosh_automation::trigger::Trigger::new(
            "disarm.secondary",
            "disarms you and sends your weapon flying",
            TriggerAction::Gag,
        );
        preset.preset = Some("disarm_buff_fade".into());
        store.set(preset.clone()).unwrap();
        assert_eq!(merge_triggers(&mut store, &r.triggers, &[]), clash);
        assert_eq!(store.get("disarm.secondary"), Some(&preset));
    }

    #[test]
    fn mudlet_key_to_macro() {
        // Qt::Key_F1 = 0x01000030, no modifier
        let xml = r#"<MudletPackage>
            <KeyPackage>
                <Key isActive="yes">
                    <name>fkick</name>
                    <script>send("kick")</script>
                    <keyCode>16777264</keyCode>
                    <keyModifier>0</keyModifier>
                </Key>
            </KeyPackage>
        </MudletPackage>"#;
        let r = parse_mudlet(xml);
        assert_eq!(r.macros.len(), 1);
        assert_eq!(r.macros[0].key, "F1");
        assert_eq!(r.macros[0].command, "kick");
    }
}
