//! The CMUD importer, for exports from CMUD and `zMUD`.

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use vosh_automation::alias::Alias;
use vosh_automation::trigger::{Trigger, TriggerAction};

use super::{attr, ImportReport};
use crate::profile::live::Macro;

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
// pattern is taken verbatim. A pattern that reads a CMUD variable,
// `@var` or the string list `{@list}`, lands as it is, and the report
// names it under the kind `trigger-pattern`, since Vosh cannot read the
// variable and the pattern never matches. Action bodies are imported
// as-is — the CMUD command language (`#CW`, `#IF`, `%1`, `@var`, etc.)
// does not translate to vosh's send-action template, so users will need
// to adjust anything beyond a plain command string.

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

pub(super) fn parse_cmud(text: &str) -> ImportReport {
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
        script: None,
    });
}

fn commit_cmud_trigger(t: CmudTriggerInProgress, report: &mut ImportReport) {
    if t.pattern.is_empty() {
        return;
    }
    let name = t
        .name
        .unwrap_or_else(|| format!("imported_{}", report.triggers.len() + 1));
    // The trigger still lands, so you can fix its pattern in place, and
    // the report says it never matches as it stands.
    let vars = cmud_pattern_vars(&t.pattern);
    if !vars.is_empty() {
        report.unsupported.push((
            "trigger-pattern".into(),
            format!("{name} (Vosh cannot read {})", name_list(&vars)),
        ));
    }
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
        patterns: vec![vosh_automation::trigger::TriggerPattern::regex(pattern)],
        priority: t.priority,
        enabled: t.enabled,
        actions,
        preset: None,
        group: None,
        target: vosh_automation::trigger::TriggerTarget::Line,
        alert: None,
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
            enabled: true,
        }),
        None => report
            .unsupported
            .push(("macro-key".into(), format!("{} -> {}", m.key_raw, m.value))),
    }
}

/// The CMUD variables a trigger pattern reads, as written and once each,
/// `@name` for a variable and `{@name}` for a string list. CMUD fills
/// them in as it matches. Vosh cannot, so the pattern keeps them as text
/// the game never sends. A `~` or a `\` before the `@` quotes it as text.
fn cmud_pattern_vars(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '~' | '\\' => i += 2,
            '@' if chars
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') =>
            {
                let start = i + 1;
                let mut end = start;
                while chars
                    .get(end)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    end += 1;
                }
                let name: String = chars[start..end].iter().collect();
                let listed = i > 0 && chars[i - 1] == '{' && chars.get(end) == Some(&'}');
                let written = if listed {
                    format!("{{@{name}}}")
                } else {
                    format!("@{name}")
                };
                if !out.contains(&written) {
                    out.push(written);
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// Names in a sentence, with a serial comma before the last of three or
/// more.
fn name_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
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
///   ~X        -> X             (literal, escaped only if X is a regex special)
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
    // `i` is a byte offset that always sits on a char boundary, so a
    // letter like "é" passes through whole.
    let mut i = 0;
    while i < bytes.len() {
        let ch = char_at(pattern, i);
        match ch {
            '~' if i + 1 < bytes.len() => {
                // CMUD quotes the next char as a literal. Escape it only
                // when the regex engine needs that, because a backslash
                // before a letter or a non-ASCII char is a different token
                // or a parse error.
                let esc = char_at(pattern, i + 1);
                out.push_str(&regex::escape(esc.encode_utf8(&mut [0; 4])));
                i += 1 + esc.len_utf8();
                continue;
            }
            '%' if i + 1 < bytes.len() => {
                let nxt = char_at(pattern, i + 1);
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
                i += 1 + nxt.len_utf8();
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
        i += ch.len_utf8();
    }
    out
}

/// The char that starts at byte offset `i`, which must be a char
/// boundary inside `s`.
fn char_at(s: &str, i: usize) -> char {
    s[i..].chars().next().expect("offset inside the string")
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let leftover = &r.macros;
        assert!(leftover.is_empty(), "{leftover:?}");
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
    fn cmud_trigger_keeps_accented_letters_whole() {
        // Casting each byte to a char turned "é" into "Ã©".
        let xml = r"<cmud><window>
          <trigger>
            <pattern>%w says héllo to {Zoë|Chloé} at 5%ü</pattern>
          </trigger>
        </window></cmud>";
        let r = parse_cmud(xml);
        assert_eq!(r.triggers.len(), 1);
        assert_eq!(
            r.triggers[0].first_pattern(),
            "(\\w+) says héllo to (?:Zoë|Chloé) at 5%ü"
        );
    }

    #[test]
    fn cmud_escape_with_tilde() {
        let translated = translate_cmud_wildcards("Spell~: foo");
        assert_eq!(translated, "Spell: foo");
        let re = regex::Regex::new(&translated).unwrap();
        assert!(re.is_match("Spell: foo"));
    }

    #[test]
    fn cmud_tilde_quotes_any_char_as_a_literal() {
        // A tilde before "é" used to emit a backslash the regex engine
        // rejects, and one before "<" or "n" became an assertion or a
        // newline instead of the plain char.
        let translated = translate_cmud_wildcards("caf~é ~* ~<~n>");
        assert_eq!(translated, "café \\* <n>");
        let re = regex::Regex::new(&translated).unwrap();
        assert!(re.is_match("café * <n>"));
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
    fn a_pattern_with_a_cmud_variable_lands_and_the_report_says_vosh_cannot_read_it() {
        let xml = r#"<cmud><window>
          <trigger name="target_dead">
            <pattern>@target is DEAD!!</pattern>
            <value>get all corpse</value>
          </trigger>
          <trigger name="enemy_in" regex="true">
            <pattern>^{@enemies} walks in\.$</pattern>
          </trigger>
          <trigger>
            <pattern>@a and @b and @a with {@c}</pattern>
          </trigger>
          <trigger name="quoted">
            <pattern>~@target is DEAD!!</pattern>
          </trigger>
          <trigger name="plain">
            <pattern>%w is DEAD!!</pattern>
          </trigger>
        </window></cmud>"#;
        let r = parse_cmud(xml);
        // Every trigger still lands, so you can fix it in place.
        assert_eq!(r.triggers.len(), 5);
        assert_eq!(r.triggers[0].first_pattern(), "@target is DEAD!!");
        assert_eq!(
            r.unsupported,
            [
                (
                    "trigger-pattern".to_string(),
                    "target_dead (Vosh cannot read @target)".to_string()
                ),
                (
                    "trigger-pattern".to_string(),
                    "enemy_in (Vosh cannot read {@enemies})".to_string()
                ),
                (
                    "trigger-pattern".to_string(),
                    "imported_3 (Vosh cannot read @a, @b, and {@c})".to_string()
                ),
            ]
        );
    }

    #[test]
    fn cmud_pattern_vars_skip_quoted_and_bare_at_signs() {
        assert_eq!(cmud_pattern_vars("@target"), ["@target"]);
        assert_eq!(cmud_pattern_vars("{@list} @list"), ["{@list}", "@list"]);
        let leftover = &cmud_pattern_vars("~@target \\@target @ @1 a@");
        assert!(leftover.is_empty(), "{leftover:?}");
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
