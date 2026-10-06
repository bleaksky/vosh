//! The GMUD importer, for the plain text config of the Java MUD client.

use vosh_automation::alias::Alias;

use super::ImportReport;
use crate::profile::live::Macro;

// gmud.cfg-style plain text. One directive per line:
//   alias [name] [command]
//   macro [F1] [say hello]
//   variable [name] [value]
//
// Square brackets are literal — gmud uses them to delimit each
// argument. Whitespace inside brackets is preserved.

pub(super) fn parse_gmud(text: &str) -> ImportReport {
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
                    script: None,
                });
            }
            ("macro", [key, command]) if !key.is_empty() && !command.is_empty() => {
                if let Some(canonical) = gmud_key_to_canonical(key) {
                    report.macros.push(Macro {
                        key: canonical,
                        command: command.clone(),
                        group: None,
                        enabled: true,
                        preset: None,
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

#[cfg(test)]
mod tests {
    use super::*;

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
