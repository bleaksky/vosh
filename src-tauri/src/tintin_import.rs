//! Tiny `TinTin++` script importer. Phase 9.
//!
//! Pulls `#alias` and `#variable` definitions out of a `.tin` file and
//! reports anything else as unsupported. The grammar is just enough to
//! cover the common Aabahran setup files; sophisticated `TinTin++`
//! features like nested events, functions, and tickers are not parsed
//! and are listed in the import report so the user can port them by
//! hand.

use std::path::Path;

use mudclient_alias::Alias;

#[derive(Debug, Default)]
pub(crate) struct ImportReport {
    pub aliases: Vec<Alias>,
    pub vars: Vec<(String, String)>,
    /// Lines we recognized but cannot model yet (event, ticker, function,
    /// etc). Each entry is `(directive, source_line)`.
    pub unsupported: Vec<(String, String)>,
    /// Lines that started with `#` but did not match any recognized
    /// directive form. Useful for catching typos.
    pub unparsed: Vec<String>,
}

pub(crate) fn import_file(path: &Path) -> std::io::Result<ImportReport> {
    let text = std::fs::read_to_string(path)?;
    Ok(parse(&text))
}

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
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }
    let mut depth = 1;
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'}' || next == b'{' || next == b'\\' {
                out.push(next as char);
                i += 2;
                continue;
            }
        }
        if b == b'{' {
            depth += 1;
            out.push('{');
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some((out, &input[i + 1..]));
            }
            out.push('}');
        } else {
            out.push(b as char);
        }
        i += 1;
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
    fn malformed_alias_goes_to_unparsed() {
        let r = parse("#alias only_one_brace");
        assert_eq!(r.unparsed.len(), 1);
    }
}
