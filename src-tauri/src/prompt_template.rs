//! ANSI renderer for the custom prompt template.
//!
//! A faithful port of the frontend renderer (src/lib/promptTemplate.ts),
//! moved backend-side so the gag-erase and the rendered replacement go out
//! in the SAME output batch. Rendering in the frontend put an IPC round
//! trip between the erase and the redraw, which flashed a blank row and
//! shifted content up for a frame on every prompt.
//!
//! Token grammar (same `%name`, `%{name}`, `%%` semantics as the vitals
//! template):
//!
//!   %name               plain substitution; unknown vars render as the
//!                       raw literal so the user spots their typo
//!   `%name_bar[:W[:C]]` auto-colored bar, width W (default 10), color C
//!                       (`auto` picks green/yellow/red by percent)
//!   `%pct_name`         current/max as an integer percent
//!   %c_<spec> %{c:...}  foreground color until reset: named color, 256
//!                       index, `r,g,b`, `#rrggbb`, or a stat name to
//!                       auto-color by its percent
//!   %bg_<spec>          background color, same spec grammar
//!   %s_<style>          bold dim italic underline inverse strike reset
//!   %time %date         local clock, HH:MM:SS / YYYY-MM-DD
//!
//! `name` is the current value; `mname` (or `max_name`, `name_max`,
//! `maxname`) is the cap. Output is reset-terminated so an unclosed color
//! cannot bleed into the server output that follows.

// Bar widths are tiny integers (capped at 80); the float math is exact.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;

const RESET: &str = "\x1b[0m";

/// Named colors as 256-palette indices, so the same name drives both
/// foreground (38;5) and background (48;5).
fn named_idx(name: &str) -> Option<u8> {
    Some(match name {
        "green" => 42,
        "red" => 196,
        "yellow" => 220,
        "blue" => 39,
        "cyan" => 51,
        "magenta" => 201,
        "white" => 255,
        "gray" => 240,
        _ => return None,
    })
}

fn color_fg(name: &str) -> Option<String> {
    named_idx(name).map(|idx| format!("\x1b[38;5;{idx}m"))
}

/// SGR attribute codes for the text-style directives.
fn style_sgr(name: &str) -> Option<&'static str> {
    Some(match name {
        "bold" => "\x1b[1m",
        "dim" => "\x1b[2m",
        "italic" => "\x1b[3m",
        "underline" | "under" => "\x1b[4m",
        "inverse" | "inv" => "\x1b[7m",
        "strike" => "\x1b[9m",
        "reset" => RESET,
        _ => return None,
    })
}

type Vars = BTreeMap<String, String>;

fn parse_number(value: Option<&String>) -> Option<f64> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn lookup_max(vars: &Vars, name: &str) -> Option<f64> {
    for key in [
        format!("m{name}"),
        format!("{name}_max"),
        format!("max_{name}"),
        format!("max{name}"),
    ] {
        if let Some(n) = parse_number(vars.get(&key)) {
            return Some(n);
        }
    }
    None
}

fn color_for_percent(pct: f64) -> String {
    let name = if pct >= 0.66 {
        "green"
    } else if pct >= 0.33 {
        "yellow"
    } else {
        "red"
    };
    color_fg(name).unwrap_or_default()
}

fn render_bar(value: f64, max: f64, width: usize, color: &str) -> String {
    let width = if width < 1 { 10 } else { width };
    let empty_fg = color_fg("gray").unwrap_or_default();
    if max <= 0.0 {
        return format!("{empty_fg}{}{RESET}", "░".repeat(width));
    }
    let pct = (value / max).clamp(0.0, 1.0);
    let filled = (pct * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let fg = if color == "auto" {
        color_for_percent(pct)
    } else {
        color_fg(color)
            .or_else(|| color_fg("green"))
            .unwrap_or_default()
    };
    format!(
        "{fg}{}{empty_fg}{}{RESET}",
        "█".repeat(filled),
        "░".repeat(empty)
    )
}

fn base_from_bar_name(name: &str) -> Option<&str> {
    if let Some(base) = name.strip_suffix("_bar") {
        return (!base.is_empty()).then_some(base);
    }
    if let Some(base) = name.strip_prefix("bar_") {
        return (!base.is_empty()).then_some(base);
    }
    None
}

/// Consume `:width:color` parameters from the start of `tail`. Returns
/// (width, color, rest). Both parameters are optional.
fn consume_bar_params(tail: &str) -> (usize, String, &str) {
    let (mut width, mut color) = (10usize, "auto".to_string());
    let Some(after_colon) = tail.strip_prefix(':') else {
        return (width, color, tail);
    };
    let digits: String = after_colon
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let mut consumed = 1 + digits.len();
    if !digits.is_empty() {
        if let Ok(w) = digits.parse::<usize>() {
            if w > 0 {
                width = w.min(80);
            }
        }
    }
    let rest_after_width = &tail[consumed..];
    if let Some(after_second) = rest_after_width.strip_prefix(':') {
        let letters: String = after_second
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        if !letters.is_empty() {
            color = letters.to_ascii_lowercase();
            consumed += 1 + letters.len();
        }
    }
    (width, color, &tail[consumed..])
}

/// Resolve a color spec to an SGR sequence for foreground (38) or, when
/// `bg`, background (48). Accepts a named color, `reset`, a 256-palette
/// index, truecolor `r,g,b` or `#rrggbb`/`rrggbb`, or a stat name to
/// auto-color by its percent. None when unrecognized.
fn color_code_from_spec(spec: &str, vars: &Vars, bg: bool) -> Option<String> {
    let p = if bg { "48" } else { "38" };
    if spec == "reset" {
        return Some(RESET.to_string());
    }
    if let Some(idx) = named_idx(spec) {
        return Some(format!("\x1b[{p};5;{idx}m"));
    }
    if spec.len() <= 3 && !spec.is_empty() && spec.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(n) = spec.parse::<u8>() {
            return Some(format!("\x1b[{p};5;{n}m"));
        }
    }
    let hex = spec.strip_prefix('#').unwrap_or(spec);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let channel = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
        let (r, g, b) = (
            channel(&hex[0..2]),
            channel(&hex[2..4]),
            channel(&hex[4..6]),
        );
        return Some(format!("\x1b[{p};2;{r};{g};{b}m"));
    }
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() == 3
        && parts
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
    {
        let clamp = |s: &str| s.parse::<u32>().unwrap_or(0).min(255);
        let (r, g, b) = (clamp(parts[0]), clamp(parts[1]), clamp(parts[2]));
        return Some(format!("\x1b[{p};2;{r};{g};{b}m"));
    }
    let value = parse_number(vars.get(spec))?;
    let max = lookup_max(vars, spec)?;
    if max > 0.0 {
        let pct = (value / max).clamp(0.0, 1.0);
        let idx = if pct >= 0.66 {
            named_idx("green")
        } else if pct >= 0.33 {
            named_idx("yellow")
        } else {
            named_idx("red")
        }?;
        return Some(format!("\x1b[{p};5;{idx}m"));
    }
    None
}

fn current_time() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn current_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn render_plain_token(name: &str, raw: &str, vars: &Vars) -> String {
    // Color directives; the spec may be a named color, 256 index, r,g,b,
    // #rrggbb, or a stat name (auto-color by its percent).
    if let Some(spec) = name.strip_prefix("c_").or_else(|| name.strip_prefix("c:")) {
        return color_code_from_spec(spec, vars, false).unwrap_or_else(|| raw.to_string());
    }
    if let Some(spec) = name
        .strip_prefix("bg_")
        .or_else(|| name.strip_prefix("bg:"))
    {
        return color_code_from_spec(spec, vars, true).unwrap_or_else(|| raw.to_string());
    }
    if let Some(style) = name.strip_prefix("s_").or_else(|| name.strip_prefix("s:")) {
        return style_sgr(style).map_or_else(|| raw.to_string(), str::to_string);
    }
    if name == "time" {
        return current_time();
    }
    if name == "date" {
        return current_date();
    }
    if let Some(base) = name.strip_prefix("pct_") {
        if let (Some(value), Some(max)) = (parse_number(vars.get(base)), lookup_max(vars, base)) {
            if max > 0.0 {
                return format!("{}", ((value / max) * 100.0).round() as i64);
            }
        }
        return raw.to_string();
    }
    vars.get(name).cloned().unwrap_or_else(|| raw.to_string())
}

enum Segment {
    Text(String),
    Token { name: String, raw: String },
}

/// Tokenize the template: `%name` (greedy `[a-z0-9_]`), `%{name}` (explicit
/// boundary, also allowing `:`/`,`/`#` for color directives), and `%%` for
/// a literal percent. Anything malformed stays literal.
fn tokenize(template: &str) -> Vec<Segment> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = Vec::new();
    let mut buffer = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            buffer.push(chars[i]);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'%') {
            buffer.push('%');
            i += 2;
            continue;
        }
        if chars.get(i + 1) == Some(&'{') {
            if let Some(close_rel) = chars[i + 2..].iter().position(|&c| c == '}') {
                let close = i + 2 + close_rel;
                let raw_name: String = chars[i + 2..close].iter().collect();
                let valid = !raw_name.is_empty()
                    && raw_name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | ',' | '#'));
                if valid {
                    if !buffer.is_empty() {
                        out.push(Segment::Text(std::mem::take(&mut buffer)));
                    }
                    out.push(Segment::Token {
                        name: raw_name.to_ascii_lowercase(),
                        raw: chars[i..=close].iter().collect(),
                    });
                    i = close + 1;
                    continue;
                }
            }
            buffer.push('%');
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
            j += 1;
        }
        if j == i + 1 {
            buffer.push('%');
            i += 1;
            continue;
        }
        if !buffer.is_empty() {
            out.push(Segment::Text(std::mem::take(&mut buffer)));
        }
        out.push(Segment::Token {
            name: chars[i + 1..j]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase(),
            raw: chars[i..j].iter().collect(),
        });
        i = j;
    }
    if !buffer.is_empty() {
        out.push(Segment::Text(buffer));
    }
    out
}

/// Render the custom prompt template against the captured prompt vars.
/// Empty template renders empty; non-empty output is reset-terminated so an
/// unclosed color cannot bleed into the server output that follows.
pub(crate) fn render_prompt_template(template: &str, vars: &Vars) -> String {
    if template.is_empty() {
        return String::new();
    }
    let mut segments = tokenize(template);
    let mut out = String::new();
    let mut i = 0;
    while i < segments.len() {
        match &segments[i] {
            Segment::Text(text) => out.push_str(text),
            Segment::Token { name, raw } => {
                if let Some(bar_base) = base_from_bar_name(name) {
                    // The tokenizer stops at non-alphanumeric, so any
                    // `:width:color` parameters landed in the next text
                    // segment. Consume the matched prefix and put the rest
                    // back in the stream.
                    let bar_base = bar_base.to_string();
                    let raw = raw.clone();
                    let mut width = 10usize;
                    let mut color = "auto".to_string();
                    if let Some(Segment::Text(next)) = segments.get(i + 1) {
                        let (w, c, rest) = consume_bar_params(next);
                        width = w;
                        color = c;
                        let rest = rest.to_string();
                        segments[i + 1] = Segment::Text(rest);
                    }
                    let value = parse_number(vars.get(&bar_base));
                    let max = lookup_max(vars, &bar_base);
                    if let (Some(value), Some(max)) = (value, max) {
                        out.push_str(&render_bar(value, max, width, &color));
                    } else {
                        out.push_str(&raw);
                    }
                } else {
                    out.push_str(&render_plain_token(name, raw, vars));
                }
            }
        }
        i += 1;
    }
    // Always reset-terminate so an unclosed color (e.g. a template still
    // being typed, before %c_reset is added) cannot bleed into the server
    // output that follows the prompt.
    if out.is_empty() {
        out
    } else {
        out + RESET
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn substitutes_vars_and_keeps_unknowns_literal() {
        let v = vars(&[("hp", "329"), ("mhp", "400")]);
        assert_eq!(
            render_prompt_template("%hp/%mhp %nope", &v),
            format!("329/400 %nope{RESET}")
        );
    }

    #[test]
    fn renders_percent_tokens() {
        let v = vars(&[("hp", "100"), ("mhp", "400")]);
        assert_eq!(render_prompt_template("%pct_hp", &v), format!("25{RESET}"));
    }

    #[test]
    fn color_specs_cover_named_256_hex_rgb_and_stat() {
        let v = vars(&[("hp", "400"), ("mhp", "400")]);
        assert_eq!(
            render_prompt_template("%c_red", &v),
            format!("\x1b[38;5;196m{RESET}")
        );
        assert_eq!(
            render_prompt_template("%{c:196}", &v),
            format!("\x1b[38;5;196m{RESET}")
        );
        assert_eq!(
            render_prompt_template("%{c:#ff8800}", &v),
            format!("\x1b[38;2;255;136;0m{RESET}")
        );
        assert_eq!(
            render_prompt_template("%{c:255,128,0}", &v),
            format!("\x1b[38;2;255;128;0m{RESET}")
        );
        // Full hp auto-colors green (index 42).
        assert_eq!(
            render_prompt_template("%c_hp", &v),
            format!("\x1b[38;5;42m{RESET}")
        );
    }

    #[test]
    fn background_and_styles_render() {
        let v = Vars::new();
        assert_eq!(
            render_prompt_template("%{bg:#330033}", &v),
            format!("\x1b[48;2;51;0;51m{RESET}")
        );
        assert_eq!(
            render_prompt_template("%s_bold", &v),
            format!("\x1b[1m{RESET}")
        );
        assert_eq!(
            render_prompt_template("%s_italic", &v),
            format!("\x1b[3m{RESET}")
        );
    }

    #[test]
    fn bars_take_width_and_color_params() {
        let v = vars(&[("hp", "200"), ("mhp", "400")]);
        let out = render_prompt_template("%hp_bar:4:green after", &v);
        // Half full at width 4: two filled, two empty, then the literal.
        assert_eq!(
            out,
            format!("\x1b[38;5;42m██\x1b[38;5;240m░░{RESET} after{RESET}")
        );
    }

    #[test]
    fn double_percent_escapes_and_unclosed_color_is_reset_terminated() {
        let v = vars(&[("hp", "10")]);
        assert_eq!(render_prompt_template("%%", &v), format!("%{RESET}"));
        let out = render_prompt_template("%{c:100,100,100}[", &v);
        assert!(out.ends_with(&format!("[{RESET}")));
    }

    #[test]
    fn empty_template_renders_empty() {
        assert_eq!(render_prompt_template("", &Vars::new()), "");
    }
}
