//! Multi-format MUD client config importer.
//!
//! Reads config files from other MUD clients and converts the bits
//! we can model into vosh's Alias / Trigger / Macro / variable
//! types. Anything we recognize but cannot represent goes into the
//! `unsupported` bucket; lines we do not understand at all go into
//! `unparsed` so the user can port them by hand.
//!
//! Supported formats:
//!   - **`MUSHclient`** world files (`.mcl`, XML rooted at `<muclient>`)
//!   - **Mudlet** package exports (`.xml`, rooted at `<MudletPackage>`)
//!   - **GMUD** plain-text config (`gmud.cfg`-style line directives)
//!
//! `TinTin`++ already has its own importer in `tintin_import.rs`; that
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
    Cmud,
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
        if head.contains("<cmud") {
            return Some(ImportFormat::Cmud);
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
        ImportFormat::Cmud => parse_cmud(text),
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
        patterns: vec![vosh_trigger::TriggerPattern {
            pattern,
            enabled: true,
        }],
        priority: sequence,
        enabled,
        group: None,
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

// Stack of in-progress items + the text-bearing child currently being
// collected. Mudlet items are hierarchical (Trigger, Alias, Key contain
// <name>, <script>, <regex>, etc.), so we accumulate text between
// matching Start/End pairs.
enum MudletStackItem {
    Trigger(MudletItem),
    Alias(MudletItem),
    Key(MudletItem),
}

fn parse_mudlet(text: &str) -> ImportReport {
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
        patterns: vec![vosh_trigger::TriggerPattern {
            pattern: item.pattern,
            enabled: true,
        }],
        priority: 100,
        enabled: item.is_active,
        actions,
        preset: None,
        group: None,
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
        let Some((directive, rest)) = line.split_once(' ') else {
            report.unparsed.push(raw.to_string());
            continue;
        };
        let args = parse_bracketed(rest);
        match (directive.to_ascii_lowercase().as_str(), args.as_slice()) {
            ("alias", [name, command]) if !name.is_empty() && !command.is_empty() => {
                report.aliases.push(Alias {
                    name: name.clone(),
                    expansion: command.clone(),
                    enabled: true,
                    group: None,
                });
            }
            ("macro", [key, command]) if !key.is_empty() && !command.is_empty() => {
                if let Some(canonical) = gmud_key_to_canonical(key) {
                    report.macros.push(Macro {
                        key: canonical,
                        command: command.clone(),
                        group: None,
                    });
                } else {
                    report
                        .unsupported
                        .push(("macro-key".into(), format!("{key} -> {command}")));
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
// CMUD / zMUD
// ===========================================================
//
// CMUD exports are XML rooted at `<cmud>` wrapping a `<window>` with
// nested `<class>` blocks. Items are children of classes (or the
// window itself):
//
//   <cmud>
//     <window name="World">
//       <class name="Combat" enabled="true">
//         <alias name="kk" enabled="true">
//           <value>kill ${target}</value>
//         </alias>
//         <trigger priority="100" regex="true">
//           <pattern>^You hit (.*)</pattern>
//           <value>say I hit %1</value>
//         </trigger>
//         <var name="target">orc</var>
//         <macro key="F1">
//           <value>look</value>
//         </macro>
//       </class>
//     </window>
//   </cmud>
//
// Class hierarchy is flattened on import (last-write-wins on alias
// name collision). Trigger patterns get a best-effort wildcard
// translation unless `regex="true"` is set, in which case the
// pattern is taken verbatim. Action bodies are imported as-is — the
// CMUD command language (`#CW`, `#IF`, `%1`, `@var`, etc.) does not
// translate to vosh's send-action template, so users will need to
// adjust anything beyond a plain command string.

#[derive(Debug, Default)]
struct CmudAliasInProgress {
    name: String,
    enabled: bool,
    value: String,
}

#[derive(Debug, Default)]
struct CmudTriggerInProgress {
    name: Option<String>,
    priority: i32,
    regex: bool,
    enabled: bool,
    pattern: String,
    value: String,
}

#[derive(Debug, Default)]
struct CmudMacroInProgress {
    key_raw: String,
    value: String,
}

#[derive(Debug, Default)]
struct CmudVarInProgress {
    name: String,
}

enum CmudItem {
    Alias(CmudAliasInProgress),
    Trigger(CmudTriggerInProgress),
    Macro(CmudMacroInProgress),
    Var(CmudVarInProgress),
}

/// Children of CMUD items whose inner text we capture.
#[derive(Debug, Clone, Copy)]
enum CmudTextTarget {
    /// `<value>...</value>` body of an alias / trigger / macro.
    Value,
    /// `<pattern>...</pattern>` body of a trigger.
    Pattern,
    /// Inner text of a `<var name="...">value</var>` element.
    VarBody,
}

fn parse_cmud(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let mut reader = Reader::from_str(text);
    // Triggers carry indented multi-line action blocks inside CDATA;
    // do not collapse that whitespace away.
    reader.config_mut().trim_text(false);

    let mut stack: Vec<CmudItem> = Vec::new();
    let mut text_target: Option<CmudTextTarget> = None;
    let mut current_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"alias" => {
                    let name = attr(&e, b"name").unwrap_or_default();
                    let enabled = cmud_enabled_attr(&e);
                    stack.push(CmudItem::Alias(CmudAliasInProgress {
                        name,
                        enabled,
                        value: String::new(),
                    }));
                }
                b"trigger" => {
                    let name = attr(&e, b"name").filter(|s| !s.is_empty());
                    let priority = attr(&e, b"priority")
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(100);
                    let regex = attr(&e, b"regex").is_some_and(|v| cmud_truthy(&v));
                    let enabled = cmud_enabled_attr(&e);
                    stack.push(CmudItem::Trigger(CmudTriggerInProgress {
                        name,
                        priority,
                        regex,
                        enabled,
                        pattern: String::new(),
                        value: String::new(),
                    }));
                }
                b"macro" => {
                    let key_raw = attr(&e, b"key").unwrap_or_default();
                    stack.push(CmudItem::Macro(CmudMacroInProgress {
                        key_raw,
                        value: String::new(),
                    }));
                }
                b"var" => {
                    let name = attr(&e, b"name").unwrap_or_default();
                    stack.push(CmudItem::Var(CmudVarInProgress { name }));
                    // var elements may be self-closing (handled in Empty)
                    // or carry inline text content. Collect either way.
                    text_target = Some(CmudTextTarget::VarBody);
                    current_text.clear();
                }
                b"value" => {
                    // Inside an alias/trigger/macro, this is the action
                    // body. Otherwise the tag is meaningless to us.
                    if matches!(
                        stack.last(),
                        Some(CmudItem::Alias(_) | CmudItem::Trigger(_) | CmudItem::Macro(_))
                    ) {
                        text_target = Some(CmudTextTarget::Value);
                        current_text.clear();
                    }
                }
                b"pattern" => {
                    if matches!(stack.last(), Some(CmudItem::Trigger(_))) {
                        text_target = Some(CmudTextTarget::Pattern);
                        current_text.clear();
                    }
                }
                b"notes" => {
                    // Capture and drop; never surfaced to the user.
                    text_target = None;
                    current_text.clear();
                }
                // `class`, `cmud`, `window`: structural only — names are
                // flattened on import. Falls through to the wildcard arm.
                b"button" | b"event" | b"func" | b"menu" | b"path" | b"stat" | b"tab"
                | b"status" | b"gauge" => {
                    let kind = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let label = attr(&e, b"name").unwrap_or_default();
                    report.unsupported.push((kind, label));
                }
                b"dir" => {
                    // Direction definitions. Vosh has no concept yet.
                    let name = attr(&e, b"name").unwrap_or_default();
                    report.unsupported.push(("dir".into(), name));
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                // Self-closing var (no body): commit immediately.
                b"var" => {
                    if let Some(name) = attr(&e, b"name") {
                        if !name.is_empty() {
                            report.vars.push((name, String::new()));
                        }
                    }
                }
                // Self-closing `alias` / `trigger` / `macro` happen in
                // sparse configs (no body yet); self-closing `class`
                // and `notes` carry no payload. All fall through to
                // the wildcard arm as no-ops.
                b"button" | b"event" | b"func" | b"menu" | b"path" | b"stat" | b"tab"
                | b"status" | b"gauge" => {
                    let kind = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let label = attr(&e, b"name").unwrap_or_default();
                    report.unsupported.push((kind, label));
                }
                b"dir" => {
                    let name = attr(&e, b"name").unwrap_or_default();
                    report.unsupported.push(("dir".into(), name));
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if text_target.is_some() {
                    current_text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(Event::CData(t)) => {
                if text_target.is_some() {
                    current_text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"value" | b"pattern" => {
                    if let Some(target) = text_target.take() {
                        let captured = current_text.trim().to_string();
                        current_text.clear();
                        if let Some(top) = stack.last_mut() {
                            match (top, target) {
                                (CmudItem::Alias(a), CmudTextTarget::Value) => a.value = captured,
                                (CmudItem::Trigger(t), CmudTextTarget::Value) => {
                                    t.value = captured;
                                }
                                (CmudItem::Trigger(t), CmudTextTarget::Pattern) => {
                                    t.pattern = captured;
                                }
                                (CmudItem::Macro(m), CmudTextTarget::Value) => m.value = captured,
                                _ => {}
                            }
                        }
                    }
                }
                b"alias" => {
                    if let Some(CmudItem::Alias(a)) = stack.pop() {
                        commit_cmud_alias(a, &mut report);
                    }
                }
                b"trigger" => {
                    if let Some(CmudItem::Trigger(t)) = stack.pop() {
                        commit_cmud_trigger(t, &mut report);
                    }
                }
                b"macro" => {
                    if let Some(CmudItem::Macro(m)) = stack.pop() {
                        commit_cmud_macro(m, &mut report);
                    }
                }
                b"var" => {
                    if let Some(CmudItem::Var(v)) = stack.pop() {
                        let captured = current_text.trim().to_string();
                        current_text.clear();
                        text_target = None;
                        if !v.name.is_empty() {
                            report.vars.push((v.name, captured));
                        }
                    }
                }
                b"notes" => {
                    current_text.clear();
                    text_target = None;
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

fn commit_cmud_alias(a: CmudAliasInProgress, report: &mut ImportReport) {
    if a.name.is_empty() || a.value.is_empty() {
        return;
    }
    report.aliases.push(Alias {
        name: a.name,
        expansion: a.value,
        enabled: a.enabled,
        group: None,
    });
}

fn commit_cmud_trigger(t: CmudTriggerInProgress, report: &mut ImportReport) {
    if t.pattern.is_empty() {
        return;
    }
    let name = t
        .name
        .unwrap_or_else(|| format!("imported_{}", report.triggers.len() + 1));
    let pattern = if t.regex {
        t.pattern
    } else {
        translate_cmud_wildcards(&t.pattern)
    };
    let mut actions: Vec<TriggerAction> = Vec::new();
    if !t.value.is_empty() {
        actions.push(TriggerAction::Send { template: t.value });
    }
    report.triggers.push(Trigger {
        name,
        patterns: vec![vosh_trigger::TriggerPattern {
            pattern,
            enabled: true,
        }],
        priority: t.priority,
        enabled: t.enabled,
        actions,
        preset: None,
        group: None,
    });
}

fn commit_cmud_macro(m: CmudMacroInProgress, report: &mut ImportReport) {
    if m.value.is_empty() {
        return;
    }
    match translate_cmud_key(&m.key_raw) {
        Some(canonical) => report.macros.push(Macro {
            key: canonical,
            command: m.value,
            group: None,
        }),
        None => report
            .unsupported
            .push(("macro-key".into(), format!("{} -> {}", m.key_raw, m.value))),
    }
}

/// CMUD truthy flag. Attributes like `enabled` / `regex` accept
/// `"true"`, `"1"`, `"yes"`. Anything else is false.
fn cmud_truthy(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

/// CMUD elements default to enabled when the attribute is absent.
/// An explicit `enabled="false"` (or `0`/`no`) flips that.
fn cmud_enabled_attr(e: &BytesStart) -> bool {
    attr(e, b"enabled").map_or(true, |v| cmud_truthy(&v))
}

/// CMUD wildcard pattern -> regex. Best-effort: covers the common
/// constructs documented in the CMUD manual. Patterns that contain
/// constructs we don't translate (CMUD function calls, lookaheads
/// with mixed syntax) are likely to need manual touch-up, but the
/// trigger still lands so the user can fix in place.
///
/// Substitutions (the table is intentionally not a doc-test):
///
/// ```text
///   ~X        -> \X            (literal escape)
///   %w        -> (\w+)         (word, capturing)
///   %d        -> (\d+)         (digits, capturing)
///   %s        -> \s+           (whitespace, not captured)
///   %a        -> .             (single char)
///   %x (1-9)  -> (.*)          (numbered wildcard slot)
///   *         -> (.*)          (any text)
///   ?         -> .             (single char)
///   {a|b|c}   -> (?:a|b|c)     (alternation)
///   . + ( ) \ -> escaped       (regex specials with no CMUD meaning)
/// ```
fn translate_cmud_wildcards(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        match ch {
            '~' if i + 1 < bytes.len() => {
                out.push('\\');
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            '%' if i + 1 < bytes.len() => {
                let nxt = bytes[i + 1] as char;
                match nxt {
                    'w' | 'W' => out.push_str("(\\w+)"),
                    'd' | 'D' => out.push_str("(\\d+)"),
                    's' | 'S' => out.push_str("\\s+"),
                    'a' | 'A' => out.push('.'),
                    '1'..='9' => out.push_str("(.*)"),
                    other => {
                        out.push('%');
                        out.push(other);
                    }
                }
                i += 2;
                continue;
            }
            '*' => out.push_str("(.*)"),
            '?' => out.push('.'),
            '{' => {
                // CMUD `{a|b|c}` alternation. Translate to non-
                // capturing regex group. Falls back to literal `{`
                // if the brace block looks malformed.
                if let Some(end) = find_matching_brace(bytes, i) {
                    let inner = &pattern[i + 1..end];
                    if inner.contains('|') {
                        out.push_str("(?:");
                        out.push_str(&translate_cmud_wildcards(inner));
                        out.push(')');
                        i = end + 1;
                        continue;
                    }
                }
                out.push_str("\\{");
            }
            '}' => out.push_str("\\}"),
            // Pass-through regex specials that CMUD also uses with the
            // same semantics: ^ $ ( ) [ ] |
            '^' | '$' | '(' | ')' | '[' | ']' | '|' | '\\' => out.push(ch),
            '+' | '.' => {
                // CMUD treats these as literals.
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}

fn find_matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// CMUD key string -> vosh canonical key. CMUD uses hyphen-prefixed
/// modifiers (`CTRL-`, `ALT-`, `SHIFT-`) plus a base token that maps
/// to a function key, numpad key, or letter. Returns None for keys
/// we don't model (raw numeric scancodes etc.).
fn translate_cmud_key(raw: &str) -> Option<String> {
    let upper = raw.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = upper.as_str();
    loop {
        if let Some(after) = rest.strip_prefix("CTRL-") {
            parts.push("Ctrl");
            rest = after;
        } else if let Some(after) = rest.strip_prefix("ALT-") {
            parts.push("Alt");
            rest = after;
        } else if let Some(after) = rest.strip_prefix("SHIFT-") {
            parts.push("Shift");
            rest = after;
        } else {
            break;
        }
    }
    let base = cmud_base_key(rest)?;
    parts.push(&base);
    Some(parts.join("+"))
}

fn cmud_base_key(token: &str) -> Option<String> {
    // F-keys
    if let Some(rest) = token.strip_prefix('F') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(format!("F{n}"));
            }
        }
    }
    // Numpad number: KEY0..KEY9 -> Numpad0..Numpad9
    if let Some(rest) = token.strip_prefix("KEY") {
        if let Ok(n) = rest.parse::<u32>() {
            if n < 10 {
                return Some(format!("Numpad{n}"));
            }
        }
    }
    let mapped = match token {
        "ADD" => "NumpadAdd",
        "SUB" | "SUBTRACT" => "NumpadSubtract",
        "MUL" | "MULTIPLY" => "NumpadMultiply",
        "DIV" | "DIVIDE" => "NumpadDivide",
        "DOT" | "DECIMAL" => "NumpadDecimal",
        "RETURN" | "ENTER" => "Enter",
        "TAB" => "Tab",
        "ESC" | "ESCAPE" => "Escape",
        "BACKSPACE" | "BACK" => "Backspace",
        "DEL" | "DELETE" => "Delete",
        "INS" | "INSERT" => "Insert",
        "HOME" => "Home",
        "END" => "End",
        "PGUP" | "PAGE_UP" | "PAGEUP" => "PageUp",
        "PGDN" | "PAGE_DOWN" | "PAGEDOWN" => "PageDown",
        "UP" => "ArrowUp",
        "DOWN" => "ArrowDown",
        "LEFT" => "ArrowLeft",
        "RIGHT" => "ArrowRight",
        "SPACE" => " ",
        _ => "",
    };
    if !mapped.is_empty() {
        return Some(mapped.to_string());
    }
    // Single letter or digit -> use as-is (uppercase letter or digit).
    if token.len() == 1 {
        let ch = token.chars().next()?;
        if ch.is_ascii_alphanumeric() {
            return Some(token.to_string());
        }
    }
    None
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
            format!("Numpad{digit}")
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
        assert_eq!(r.triggers[0].first_pattern(), "^You hit");
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

    #[test]
    fn detect_format_cmud() {
        let text = "<?xml version=\"1.0\"?>\n<cmud>\n<window/>\n</cmud>\n";
        assert_eq!(detect_format(text), Some(ImportFormat::Cmud));
    }

    #[test]
    fn cmud_alias_and_var() {
        let xml = r#"<?xml version="1.0"?>
<cmud>
  <window name="W">
    <class name="Combat">
      <alias name="kk">
        <value>kill orc</value>
      </alias>
      <var name="target">orc</var>
      <var name="empty"/>
    </class>
  </window>
</cmud>"#;
        let r = parse_cmud(xml);
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].name, "kk");
        assert_eq!(r.aliases[0].expansion, "kill orc");
        assert!(r.aliases[0].enabled);
        assert_eq!(r.vars.len(), 2);
        assert_eq!(r.vars[0], ("target".into(), "orc".into()));
        assert_eq!(r.vars[1], ("empty".into(), String::new()));
    }

    #[test]
    fn cmud_trigger_wildcards_translated() {
        let xml = r#"<cmud><window>
          <trigger priority="100">
            <pattern>You hit %w for %d damage.</pattern>
            <value>say got it</value>
          </trigger>
        </window></cmud>"#;
        let r = parse_cmud(xml);
        assert_eq!(r.triggers.len(), 1);
        assert_eq!(
            r.triggers[0].first_pattern(),
            "You hit (\\w+) for (\\d+) damage\\."
        );
        assert!(matches!(
            &r.triggers[0].actions[0],
            TriggerAction::Send { template } if template == "say got it"
        ));
    }

    #[test]
    fn cmud_trigger_regex_passthrough() {
        let xml = r#"<cmud><window>
          <trigger priority="50" regex="true">
            <pattern>^Spell: (.*?) wears off$</pattern>
            <value>echo gone</value>
          </trigger>
        </window></cmud>"#;
        let r = parse_cmud(xml);
        assert_eq!(r.triggers.len(), 1);
        // regex=true skips wildcard translation; pattern is byte-for-byte.
        assert_eq!(r.triggers[0].first_pattern(), "^Spell: (.*?) wears off$");
    }

    #[test]
    fn cmud_trigger_cdata_value() {
        let xml = "<cmud><window>\n\
          <trigger priority=\"1\">\n\
            <pattern>foo</pattern>\n\
            <value><![CDATA[#LOCAL $A\n$A = %1\n#CW peru]]></value>\n\
          </trigger>\n\
        </window></cmud>";
        let r = parse_cmud(xml);
        assert_eq!(r.triggers.len(), 1);
        if let TriggerAction::Send { template } = &r.triggers[0].actions[0] {
            assert!(template.contains("#LOCAL $A"));
            assert!(template.contains("#CW peru"));
        } else {
            panic!("expected send action");
        }
    }

    #[test]
    fn cmud_macro_keys() {
        let xml = r#"<cmud><window>
          <macro key="F1"><value>look</value></macro>
          <macro key="CTRL-F12"><value>quit</value></macro>
          <macro key="KEY5"><value>look</value></macro>
          <macro key="ALT-KEY1"><value>kick</value></macro>
          <macro key="ADD"><value>up</value></macro>
        </window></cmud>"#;
        let r = parse_cmud(xml);
        let keys: Vec<&str> = r.macros.iter().map(|m| m.key.as_str()).collect();
        assert!(keys.contains(&"F1"));
        assert!(keys.contains(&"Ctrl+F12"));
        assert!(keys.contains(&"Numpad5"));
        assert!(keys.contains(&"Alt+Numpad1"));
        assert!(keys.contains(&"NumpadAdd"));
    }

    #[test]
    fn cmud_macro_unknown_key_unsupported() {
        let xml = r#"<cmud><window><macro key="171"><value>x</value></macro></window></cmud>"#;
        let r = parse_cmud(xml);
        assert!(r.macros.is_empty());
        assert!(r.unsupported.iter().any(|(k, _)| k == "macro-key"));
    }

    #[test]
    fn cmud_alias_disabled() {
        let xml = r#"<cmud><window>
          <alias name="aa" enabled="false"><value>on</value></alias>
        </window></cmud>"#;
        let r = parse_cmud(xml);
        assert_eq!(r.aliases.len(), 1);
        assert!(!r.aliases[0].enabled);
    }

    #[test]
    fn cmud_braced_alternation() {
        // {his|her|its} -> (?:his|her|its)
        let translated = translate_cmud_wildcards("{he|she|it} grabs");
        assert_eq!(translated, "(?:he|she|it) grabs");
    }

    #[test]
    fn cmud_escape_with_tilde() {
        let translated = translate_cmud_wildcards("Spell~: foo");
        assert_eq!(translated, "Spell\\: foo");
    }

    /// One-shot smoke test against a real CMUD export.
    /// Set `VOSH_CMUD_SMOKE=/path/to/file.xml` to enable. Prints a
    /// summary so we can eyeball what landed without flooding the
    /// CI logs.
    #[test]
    #[ignore = "smoke test; opt in via VOSH_CMUD_SMOKE env var"]
    fn cmud_real_file_smoke() {
        let Ok(path) = std::env::var("VOSH_CMUD_SMOKE") else {
            return;
        };
        let text = std::fs::read_to_string(&path).expect("read VOSH_CMUD_SMOKE file");
        let r = parse_cmud(&text);
        eprintln!(
            "aliases={} triggers={} vars={} macros={} unsupported={} unparsed={}",
            r.aliases.len(),
            r.triggers.len(),
            r.vars.len(),
            r.macros.len(),
            r.unsupported.len(),
            r.unparsed.len(),
        );
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for (k, _) in &r.unsupported {
            *counts.entry(k.as_str()).or_insert(0) += 1;
        }
        for (k, n) in &counts {
            eprintln!("  unsupported.{k} = {n}");
        }
        assert!(r.aliases.len() + r.triggers.len() + r.macros.len() > 0);
    }

    #[test]
    fn cmud_unsupported_elements_reported() {
        let xml = r#"<cmud><window>
          <class name="ui">
            <button name="atk"><value>kick</value></button>
            <dir name="n">north</dir>
          </class>
        </window></cmud>"#;
        let r = parse_cmud(xml);
        let kinds: Vec<&str> = r.unsupported.iter().map(|(k, _)| k.as_str()).collect();
        assert!(kinds.contains(&"button"));
        assert!(kinds.contains(&"dir"));
    }
}
