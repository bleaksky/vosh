//! Multi-format MUD client config importer.
//!
//! Reads config files from other MUD clients and converts the bits
//! we can model into vosh's Alias / Trigger / Macro / variable
//! types. Anything we recognize but cannot represent goes into the
//! `unsupported` bucket; lines we do not understand at all go into
//! `unparsed` so the user can port them by hand.
//!
//! Supported formats:
//!   - **MUSHclient** world files (`.mcl`, XML rooted at `<muclient>`)
//!   - **Mudlet** package exports (`.xml`, rooted at `<MudletPackage>`)
//!   - **GMUD** plain-text config (`gmud.cfg`-style line directives)
//!
//! TinTin++ already has its own importer in `tintin_import.rs`; that
//! module ships separately and is unaffected by this one.

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use vosh_alias::Alias;
use vosh_trigger::{Trigger, TriggerAction};

use crate::profile::Macro;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct ImportReport {
    pub aliases: Vec<Alias>,
    pub triggers: Vec<Trigger>,
    pub macros: Vec<Macro>,
    pub vars: Vec<(String, String)>,
    /// Lines or elements we recognized but cannot model (Lua
    /// scripts, plugin code, color triggers, etc). Each entry is
    /// `(kind, descriptor)` so the UI can summarize by kind.
    pub unsupported: Vec<(String, String)>,
    /// Things that did not match any expected shape — usually
    /// hints at a typo or an unfamiliar dialect.
    pub unparsed: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportFormat {
    Mushclient,
    Mudlet,
    Gmud,
}

/// Sniff the file content to guess which client it came from. The
/// frontend ships its own extension check first; this is the
/// fallback when extension is missing or ambiguous.
pub(crate) fn detect_format(text: &str) -> Option<ImportFormat> {
    let head = text.trim_start();
    if head.starts_with("<?xml") || head.starts_with('<') {
        if head.contains("<MudletPackage") {
            return Some(ImportFormat::Mudlet);
        }
        if head.contains("<muclient") || head.contains("<plugin") {
            return Some(ImportFormat::Mushclient);
        }
        return None;
    }
    // Plain-text formats. GMUD lines look like
    //   alias [name] [command]
    //   macro [F1] [command]
    for raw in head.lines().take(20) {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        if t.starts_with("alias ") || t.starts_with("macro ") || t.starts_with("variable ") {
            return Some(ImportFormat::Gmud);
        }
    }
    None
}

pub(crate) fn parse(format: ImportFormat, text: &str) -> ImportReport {
    match format {
        ImportFormat::Mushclient => parse_mushclient(text),
        ImportFormat::Mudlet => parse_mudlet(text),
        ImportFormat::Gmud => parse_gmud(text),
    }
}

// ===========================================================
// MUSHclient
// ===========================================================
//
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

fn parse_mushclient(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => match e.name().as_ref() {
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
                b"timer" => report.unsupported.push((
                    "timer".into(),
                    attr(&e, b"name").unwrap_or_default(),
                )),
                b"plugin_script" | b"script" => {
                    report.unsupported.push((
                        "script".into(),
                        attr(&e, b"name").unwrap_or_default(),
                    ));
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
    let enabled = attr(e, b"enabled")
        .map(|v| matches!(v.as_str(), "y" | "yes" | "true" | "1"))
        .unwrap_or(true);
    Some(Alias {
        name,
        expansion: send,
        enabled,
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
    let enabled = attr(e, b"enabled")
        .map(|v| matches!(v.as_str(), "y" | "yes" | "true" | "1"))
        .unwrap_or(true);
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
            format!("{} (palette index dropped)", name),
        ));
    }
    let mut actions: Vec<TriggerAction> = Vec::new();
    if !send.is_empty() {
        actions.push(TriggerAction::Send { template: send });
    }
    Some(Trigger {
        name,
        pattern,
        priority: sequence,
        enabled,
        actions,
        preset: None,
    })
}

// ===========================================================
// Mudlet
// ===========================================================
//
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

fn parse_mudlet(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    // Stack of in-progress items + the text-bearing child currently
    // being collected. Mudlet items are hierarchical (Trigger,
    // Alias, Key contain <name>, <script>, <regex>, etc.), so we
    // accumulate text between matching Start/End pairs.
    enum Item {
        Trigger(MudletItem),
        Alias(MudletItem),
        Key(MudletItem),
    }
    let mut stack: Vec<Item> = Vec::new();
    let mut text_target: Option<String> = None;
    let mut current_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "Trigger" => stack.push(Item::Trigger(MudletItem::from_attrs(&e))),
                    "Alias" => stack.push(Item::Alias(MudletItem::from_attrs(&e))),
                    "Key" => stack.push(Item::Key(MudletItem::from_attrs(&e))),
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
                                    Item::Trigger(i) | Item::Alias(i) | Item::Key(i) => i,
                                };
                                item.put(&target, current_text.clone());
                            }
                            current_text.clear();
                        }
                    }
                    "Trigger" => {
                        if let Some(Item::Trigger(item)) = stack.pop() {
                            commit_mudlet_trigger(item, &mut report);
                        }
                    }
                    "Alias" => {
                        if let Some(Item::Alias(item)) = stack.pop() {
                            commit_mudlet_alias(item, &mut report);
                        }
                    }
                    "Key" => {
                        if let Some(Item::Key(item)) = stack.pop() {
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
                .map(|v| matches!(v.as_str(), "yes" | "true" | "1"))
                .unwrap_or(true),
            ..Self::default()
        }
    }

    fn put(&mut self, field: &str, value: String) {
        match field {
            "name" => self.name = value,
            "script" => self.script = value,
            "regex" => self.pattern = value,
            "pattern" | "string" => {
                // Trigger has <regexCodeList><string>regex</string></regexCodeList>
                // Take the first non-empty pattern.
                if self.pattern.is_empty() {
                    self.pattern = value;
                }
            }
            "command" => self.script = value,
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
        pattern: item.pattern,
        priority: 100,
        enabled: item.is_active,
        actions,
        preset: None,
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
    });
}

fn commit_mudlet_key(item: MudletItem, report: &mut ImportReport) {
    let canonical = match qt_key_to_canonical(&item.key_code, &item.key_modifier) {
        Some(k) => k,
        None => {
            report
                .unsupported
                .push(("key-binding".into(), format!("{} (Qt key {})", item.name, item.key_code)));
            return;
        }
    };
    let command = extract_send_command(&item.script).unwrap_or_else(|| item.script.clone());
    if command.trim().is_empty() {
        return;
    }
    report.macros.push(Macro {
        key: canonical,
        command,
    });
}

// ===========================================================
// GMUD (Java MUD client)
// ===========================================================
//
// gmud.cfg-style plain text. One directive per line:
//   alias [name] [command]
//   macro [F1] [say hello]
//   variable [name] [value]
//
// Square brackets are literal — gmud uses them to delimit each
// argument. Whitespace inside brackets is preserved.

fn parse_gmud(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let (directive, rest) = match line.split_once(' ') {
            Some(pair) => pair,
            None => {
                report.unparsed.push(raw.to_string());
                continue;
            }
        };
        let args = parse_bracketed(rest);
        match (directive.to_ascii_lowercase().as_str(), args.as_slice()) {
            ("alias", [name, command]) if !name.is_empty() && !command.is_empty() => {
                report.aliases.push(Alias {
                    name: name.clone(),
                    expansion: command.clone(),
                    enabled: true,
                });
            }
            ("macro", [key, command]) if !key.is_empty() && !command.is_empty() => {
                if let Some(canonical) = gmud_key_to_canonical(key) {
                    report.macros.push(Macro {
                        key: canonical,
                        command: command.clone(),
                    });
                } else {
                    report
                        .unsupported
                        .push(("macro-key".into(), format!("{} -> {}", key, command)));
                }
            }
            ("variable", [name, value]) if !name.is_empty() => {
                report.vars.push((name.clone(), value.clone()));
            }
            _ => {
                report.unparsed.push(raw.to_string());
            }
        }
    }
    report
}

// Split a "[a] [b] [c]" string into its bracketed pieces. Returns
// at most 2 entries (name, value) since every GMUD directive we
// model is binary; extra trailing content gets folded into the
// last entry.
fn parse_bracketed(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_bracket = false;
    for ch in text.chars() {
        match ch {
            '[' if !in_bracket => {
                in_bracket = true;
                cur.clear();
            }
            ']' if in_bracket => {
                in_bracket = false;
                out.push(std::mem::take(&mut cur));
                if out.len() == 2 {
                    break;
                }
            }
            _ if in_bracket => cur.push(ch),
            _ => {}
        }
    }
    out
}

// ===========================================================
// Shared helpers
// ===========================================================

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
        if let Some(inner) = rest.trim().strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            return Some(inner.to_string());
        }
    }
    None
}

fn strip_quoted(s: &str) -> Option<String> {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            return Some(s[1..s.len() - 1].to_string());
        }
    }
    None
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().with_checks(false).find_map(|a| {
        let a = a.ok()?;
        if a.key == QName(name) {
            Some(String::from_utf8_lossy(&a.value).into_owned())
        } else {
            None
        }
    })
}

fn debug_tag(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
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
            format!("F{}", n)
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

/// Map a gmud macro key token (e.g. "F1", "ctrl-N", "kp7") to the
/// canonical form vosh uses.
fn gmud_key_to_canonical(token: &str) -> Option<String> {
    let mut t = token.trim().to_string();
    // GMUD writes ctrl-X / alt-X / shift-X. Reorder to canonical.
    let mut parts: Vec<&str> = Vec::new();
    let lower = t.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("ctrl-") {
        parts.push("Ctrl");
        t = rest.to_string();
    } else if let Some(rest) = lower.strip_prefix("alt-") {
        parts.push("Alt");
        t = rest.to_string();
    }
    let base = match t.to_ascii_lowercase().as_str() {
        "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => {
            t.to_ascii_uppercase()
        }
        "kp0" | "kp1" | "kp2" | "kp3" | "kp4" | "kp5" | "kp6" | "kp7" | "kp8" | "kp9" => {
            let digit = &t[2..];
            format!("Numpad{}", digit)
        }
        s if s.len() == 1 => s.to_ascii_uppercase(),
        _ => return None,
    };
    let mut s = parts.join("+");
    if !s.is_empty() {
        s.push('+');
    }
    s.push_str(&base);
    Some(s)
}

// ===========================================================
// Tests
// ===========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mudlet_by_root() {
        let text = r#"<?xml version="1.0"?><MudletPackage version="1.001"></MudletPackage>"#;
        assert_eq!(detect_format(text), Some(ImportFormat::Mudlet));
    }

    #[test]
    fn detects_mushclient_by_root() {
        let text = r#"<?xml version="1.0"?><muclient><world></world></muclient>"#;
        assert_eq!(detect_format(text), Some(ImportFormat::Mushclient));
    }

    #[test]
    fn detects_gmud_by_directives() {
        let text = "alias [g] [get $1.gold]\nmacro [F1] [say hi]\n";
        assert_eq!(detect_format(text), Some(ImportFormat::Gmud));
    }

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
        assert_eq!(r.triggers[0].pattern, "^You hit");
        assert_eq!(r.triggers[0].priority, 50);
        assert_eq!(r.triggers[0].actions.len(), 1);
    }

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
        assert_eq!(r.aliases[0].enabled, true);
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
        assert_eq!(r.triggers[0].pattern, "^You hit");
        assert_eq!(r.triggers[0].actions.len(), 1);
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

    #[test]
    fn gmud_aliases_macros_vars() {
        let text = "alias [g] [get $1.gold]\nmacro [F1] [say hi]\nvariable [tgt] [orc]\n";
        let r = parse_gmud(text);
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].name, "g");
        assert_eq!(r.macros.len(), 1);
        assert_eq!(r.macros[0].key, "F1");
        assert_eq!(r.vars.len(), 1);
        assert_eq!(r.vars[0].0, "tgt");
    }

    #[test]
    fn gmud_ctrl_prefix() {
        let text = "macro [ctrl-N] [north]\n";
        let r = parse_gmud(text);
        assert_eq!(r.macros.len(), 1);
        assert_eq!(r.macros[0].key, "Ctrl+N");
    }
}
