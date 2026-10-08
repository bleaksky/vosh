//! A scene as one HTML file that opens anywhere (Q11 of the Alerts and
//! Scenes review, board 6). A short header names the place, your
//! character and the time, the lines sit in one `pre`, and each run of one
//! SGR state is a span with a class: `c0` to `c15` for the 16 colors, `g0`
//! to `g15` for a ground in one of them, and `b`, `i` and `u` for bold,
//! italic and underline. The ground and the 16 colors of the theme
//! showing when you save are CSS variables in one style block. A 256
//! color past the 16 or a true color has no name in the palette, so it
//! goes inline. No script, no font file and no request, so the file reads
//! the same offline and in a mail preview. Every piece of text is escaped,
//! so a line of the game can never add markup.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::Deserialize;
use vosh_automation::trigger::readable::xterm256;
use vosh_protocol::ansi::{AnsiParser, Attributes, Color};

/// The theme showing when you save, as the page sends it: the ground, the
/// text, the quiet text and the 16 colors, each a CSS color. A palette
/// left empty reads as Obsidian Ember's.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ScenePalette {
    pub(crate) background: String,
    pub(crate) foreground: String,
    pub(crate) muted: String,
    pub(crate) ansi: Vec<String>,
}

impl ScenePalette {
    /// The palette with every color checked, so none can close the style
    /// block. A color that is not `#` and hex digits, or `rgb(…)`, reads as
    /// Obsidian Ember's.
    fn checked(&self) -> Self {
        const GROUND: &str = "#050403";
        const TEXT: &str = "#c0bdbb";
        const MUTED: &str = "#646260";
        let pick = |color: &str, fallback: &str| {
            if safe_color(color) {
                color.trim().to_string()
            } else {
                fallback.to_string()
            }
        };
        Self {
            background: pick(&self.background, GROUND),
            foreground: pick(&self.foreground, TEXT),
            muted: pick(&self.muted, MUTED),
            ansi: (0..16)
                .map(|i| pick(self.ansi.get(i).map_or("", String::as_str), TEXT))
                .collect(),
        }
    }
}

/// True for a color of the shapes a theme gives, `#` and 3 to 8 hex
/// digits, or `rgb(…)` and `rgba(…)` of numbers.
fn safe_color(color: &str) -> bool {
    let color = color.trim();
    if let Some(hex) = color.strip_prefix('#') {
        return (3..=8).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let inner = color
        .strip_prefix("rgba(")
        .or_else(|| color.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'));
    inner.is_some_and(|inner| {
        inner
            .chars()
            .all(|c| c.is_ascii_digit() || " ,.%".contains(c))
    })
}

/// What the header says.
pub(crate) struct Header<'a> {
    /// The page's name in the browser, the file's name without its
    /// extension.
    pub(crate) name: &'a str,
    /// The title, the first room name in the range.
    pub(crate) title: &'a str,
    /// The line under it, who, where and when.
    pub(crate) meta: &'a str,
    /// The line under the scene, what it leaves out.
    pub(crate) footer: &'a str,
}

/// `text` with `&`, `<`, `>` and `"` escaped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// The CSS color of a 256 color index or a true color, for a style. The
/// 16 are named by class.
fn inline_color(color: Color) -> Option<String> {
    match color {
        Color::Indexed256(n) => xterm256(n).map(|(r, g, b)| format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Rgb { r, g, b } => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Default | Color::Indexed16(_) => None,
    }
}

/// The palette slot of a color, when it is one of the 16.
fn slot(color: Color) -> Option<u8> {
    match color {
        Color::Indexed16(n) => Some(n),
        Color::Indexed256(n) if n < 16 => Some(n),
        _ => None,
    }
}

/// The span a run of text opens with, or None for a run in the plain
/// text color. Bold lifts the 8 colors to their bright pair, as the
/// terminal draws a MUD's bold. Inverse swaps the text and its ground.
fn open_span(attrs: &Attributes, used: &mut BTreeSet<String>) -> Option<String> {
    let (mut fg, mut bg) = (attrs.fg, attrs.bg);
    if attrs.bold {
        if let Some(n @ 0..=7) = slot(fg) {
            fg = Color::Indexed16(n + 8);
        }
    }
    let mut classes = Vec::new();
    let mut styles = Vec::new();
    if attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
        // A side the line leaves unset stands in the page's own.
        if fg == Color::Default {
            styles.push("color:var(--bg)".to_string());
        }
        if bg == Color::Default {
            styles.push("background:var(--fg)".to_string());
        }
    }
    if let Some(n) = slot(fg) {
        classes.push(format!("c{n}"));
    } else if let Some(css) = inline_color(fg) {
        styles.push(format!("color:{css}"));
    }
    if let Some(n) = slot(bg) {
        classes.push(format!("g{n}"));
    } else if let Some(css) = inline_color(bg) {
        styles.push(format!("background:{css}"));
    }
    for (on, class) in [
        (attrs.bold, "b"),
        (attrs.italic, "i"),
        (attrs.underline, "u"),
    ] {
        if on {
            classes.push(class.to_string());
        }
    }
    if classes.is_empty() && styles.is_empty() {
        return None;
    }
    used.extend(classes.iter().cloned());
    let mut open = String::from("<span");
    if !classes.is_empty() {
        let _ = write!(open, " class=\"{}\"", classes.join(" "));
    }
    if !styles.is_empty() {
        let _ = write!(open, " style=\"{}\"", styles.join(";"));
    }
    open.push('>');
    Some(open)
}

/// The rule for one class a span uses.
fn class_rule(class: &str) -> String {
    match class {
        "b" => ".b { font-weight: 700 }".to_string(),
        "i" => ".i { font-style: italic }".to_string(),
        "u" => ".u { text-decoration: underline }".to_string(),
        _ => match class.split_at(1) {
            ("c", n) => format!(".c{n} {{ color: var(--c{n}) }}"),
            (_, n) => format!(".g{n} {{ background: var(--c{n}) }}"),
        },
    }
}

/// The file for `lines`, each the bytes the game sent, colors included.
pub(crate) fn render(lines: &[Vec<u8>], header: &Header, palette: &ScenePalette) -> String {
    let palette = palette.checked();
    let mut used = BTreeSet::new();
    let mut body = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            body.push('\n');
        }
        // Each line starts plain, as the log keeps it.
        for span in AnsiParser::new().feed(line) {
            let text = escape(&span.text);
            match open_span(&span.attrs, &mut used) {
                Some(open) => {
                    let _ = write!(body, "{open}{text}</span>");
                }
                None => body.push_str(&text),
            }
        }
    }
    let mut vars = format!(
        "--bg: {}; --fg: {}; --muted: {};",
        palette.background, palette.foreground, palette.muted
    );
    for (n, color) in palette.ansi.iter().enumerate() {
        let _ = write!(vars, " --c{n}: {color};");
    }
    let rules: Vec<String> = used.iter().map(|class| class_rule(class)).collect();
    format!(
        "<!doctype html>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title_tag}</title>
<style>
:root {{ {vars} }}
body {{ margin: 0; background: var(--bg); color: var(--fg);
  font: 13px/18px -apple-system, BlinkMacSystemFont, system-ui, sans-serif; }}
main {{ max-width: 680px; margin: 0 auto; padding: 40px 24px; }}
h1 {{ margin: 0; font-size: 20px; line-height: 26px; font-weight: 600; letter-spacing: -0.01em; }}
header p {{ margin: 6px 0 0; color: var(--muted); }}
hr {{ height: 1px; margin: 20px 0; border: 0; background: var(--muted); opacity: 0.4; }}
pre {{ margin: 0; font: 13px/17px \"JetBrains Mono\", Menlo, Consolas, monospace;
  white-space: pre-wrap; }}
footer {{ margin: 20px 0 0; color: var(--muted); font-size: 12px; line-height: 16px; }}
{rules}
</style>
<main>
<header><h1>{title}</h1>
<p>{meta}</p></header>
<hr>
<pre>{body}</pre>
<footer>{footer}</footer>
</main>
",
        title_tag = escape(header.name),
        title = escape(header.title),
        meta = escape(header.meta),
        footer = escape(header.footer),
        rules = rules.join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> ScenePalette {
        ScenePalette {
            background: "#050403".into(),
            foreground: "#c0bdbb".into(),
            muted: "#646260".into(),
            ansi: (0..16).map(|n| format!("#0000{n:02x}")).collect(),
        }
    }

    fn header() -> Header<'static> {
        Header {
            name: "Thickening Woods, October 3",
            title: "Thickening Woods",
            meta: "Orla in The Forsaken Lands, October 3, 2026, from 21:14 to 21:15",
            footer: "Saved from Vosh.",
        }
    }

    #[test]
    fn each_color_run_is_a_span_with_a_class() {
        let lines = [
            b"\x1b[38;5;28m\x1b[0;32mThickening Woods\x1b[0;0m\x1b[0;0m".to_vec(),
            b"Tolliver says '\x1b[0;1;33mThe day has begun.\x1b[0;0m'".to_vec(),
        ];
        let html = render(&lines, &header(), &palette());
        assert!(html.contains(
            "<pre><span class=\"c2\">Thickening Woods</span>\nTolliver says '<span class=\"c11 b\">The day has begun.</span>'</pre>"
        ), "{html}");
        assert!(html.contains(".c2 { color: var(--c2) }"));
        assert!(html.contains(".c11 { color: var(--c11) }"));
        assert!(html.contains(".b { font-weight: 700 }"));
        assert!(!html.contains(".c1 {"), "only the classes in use");
        assert!(html.contains("--c15: #00000f;"));
        assert!(html.contains("<title>Thickening Woods, October 3</title>"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn a_256_color_past_the_16_goes_inline() {
        let html = render(
            &[b"\x1b[38;5;196mred\x1b[0m".to_vec()],
            &header(),
            &palette(),
        );
        assert!(
            html.contains("<span style=\"color:#ff0000\">red</span>"),
            "{html}"
        );
    }

    #[test]
    fn a_line_of_the_game_never_adds_markup() {
        let html = render(
            &[b"Tolliver says '<script>alert(1)</script> & more'".to_vec()],
            &Header {
                name: "</title><script>",
                title: "</title><script>",
                ..header()
            },
            &palette(),
        );
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; more"));
    }

    #[test]
    fn a_color_that_could_close_the_style_block_falls_back() {
        let mut bad = palette();
        bad.background = "red;} body{display:none".into();
        bad.ansi[2] = "</style>".into();
        let html = render(&[b"x".to_vec()], &header(), &bad);
        assert!(html.contains("--bg: #050403;"));
        assert!(html.contains("--c2: #c0bdbb;"));
        assert!(!html.contains("display:none"));
        assert!(safe_color("rgb(12, 34, 56)"));
        assert!(!safe_color("url(x)"));
    }
}
