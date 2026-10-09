//! Tiny `TinTin++` script importer.
//!
//! Pulls `#alias` and `#variable` definitions out of a `.tin` file and
//! reports anything else as unsupported. The grammar is just enough to
//! cover the common Aabahran setup files; sophisticated `TinTin++`
//! features like nested events, functions, and tickers are not parsed
//! and are listed in the import report so the user can port them by
//! hand.

use std::path::Path;

use vosh_automation::alias::Alias;

use super::ImportReport;

pub(crate) fn import_file(path: &Path) -> std::io::Result<ImportReport> {
    // TinTin++ scripts often stash raw telnet bytes (IAC, DO, etc.) inside
    // #variable values. Those bytes are not valid UTF-8, so read as bytes
    // and decode lossily; the substitute character on a non-UTF-8 byte is
    // fine for the alias and variable extraction we do here.
    let raw = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&raw);
    Ok(parse(text.as_ref()))
}

/// A `.tin` file holds no triggers or macros this reads, so those two
/// lists of the report stay empty.
pub(crate) fn parse(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("#nop") || line.starts_with(';') {
            continue;
        }
        let Some(rest) = line.strip_prefix('#') else {
            continue;
        };
        let lower = rest.to_ascii_lowercase();
        if lower.starts_with("alias") {
            if let Some(alias) = parse_braced_pair(&rest[5..]).map(|(name, expansion)| Alias {
                name,
                expansion,
                enabled: true,
                group: None,
                script: None,
            }) {
                report.aliases.push(alias);
            } else {
                report.unparsed.push(raw.to_string());
            }
        } else if lower.starts_with("variable") || lower.starts_with("var ") {
            let body_start = if lower.starts_with("variable") { 8 } else { 4 };
            if let Some(pair) = parse_braced_pair(&rest[body_start..]) {
                report.vars.push(pair);
            } else {
                report.unparsed.push(raw.to_string());
            }
        } else {
            let directive = first_word(rest);
            report.unsupported.push((directive, raw.to_string()));
        }
    }
    report
}

/// Read `{foo}{bar}` (with nested-brace tolerance and `\}` escapes).
/// Returns the two values when both blocks are present.
fn parse_braced_pair(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim_start();
    let (a, rest) = read_braced(trimmed)?;
    let rest = rest.trim_start();
    let (b, _) = read_braced(rest)?;
    Some((a, b))
}

fn read_braced(input: &str) -> Option<(String, &str)> {
    // Walk chars, not bytes, so a letter like "é" stays whole.
    let mut chars = input.char_indices().peekable();
    if chars.next()?.1 != '{' {
        return None;
    }
    let mut depth = 1;
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            if let Some(&(_, next @ ('}' | '{' | '\\'))) = chars.peek() {
                out.push(next);
                chars.next();
                continue;
            }
        }
        if c == '{' {
            depth += 1;
            out.push('{');
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Some((out, &input[i + 1..]));
            }
            out.push('}');
        } else {
            out.push(c);
        }
    }
    None
}

fn first_word(s: &str) -> String {
    s.split_whitespace().next().unwrap_or("").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_alias() {
        let r = parse("#alias {greet} {wave;bow;say hi}");
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].name, "greet");
        assert_eq!(r.aliases[0].expansion, "wave;bow;say hi");
    }

    #[test]
    fn parses_variable() {
        let r = parse("#variable {target} {goblin}");
        assert_eq!(r.vars.len(), 1);
        assert_eq!(r.vars[0], ("target".to_string(), "goblin".to_string()));
    }

    #[test]
    fn parses_var_short_form() {
        let r = parse("#var {hp} {100}");
        assert_eq!(r.vars.len(), 1);
        assert_eq!(r.vars[0], ("hp".to_string(), "100".to_string()));
    }

    #[test]
    fn ignores_comments_and_blanks() {
        let r = parse("#nop comment\n  \n#alias {x} {y}");
        assert_eq!(r.aliases.len(), 1);
    }

    #[test]
    fn unsupported_directives_listed() {
        let r = parse("#event {IAC SB GMCP} {do stuff}\n#ticker {t} {bump} {1}");
        assert_eq!(r.unsupported.len(), 2);
        assert_eq!(r.unsupported[0].0, "event");
        assert_eq!(r.unsupported[1].0, "ticker");
    }

    #[test]
    fn handles_nested_braces_in_expansion() {
        let r = parse("#alias {wrap} {echo {hello world}}");
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].expansion, "echo {hello world}");
    }

    #[test]
    fn keeps_accented_letters_whole() {
        // Casting each byte to a char turned "é" into "Ã©".
        let r = parse("#alias {grüß} {say héllo {to Zoë} \\}é}\n#variable {weapon} {épée}");
        assert_eq!(r.aliases.len(), 1);
        assert_eq!(r.aliases[0].name, "grüß");
        assert_eq!(r.aliases[0].expansion, "say héllo {to Zoë} }é");
        assert_eq!(r.vars, vec![("weapon".to_string(), "épée".to_string())]);
    }

    #[test]
    fn malformed_alias_goes_to_unparsed() {
        let r = parse("#alias only_one_brace");
        assert_eq!(r.unparsed.len(), 1);
    }
}
