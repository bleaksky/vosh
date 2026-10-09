//! Match the trigger store against a single line and produce the resulting
//! display text plus any side effects.

use std::borrow::Cow;

use regex::Regex;
#[cfg(any(test, feature = "testkit"))]
use vosh_protocol::ansi::plain_text;
use vosh_protocol::ansi::PieceKind;

use crate::alert::AlertParts;
use crate::split::split_commands;
use crate::stops::StopKey;
use crate::trigger::action::{HighlightStyle, TriggerAction};
use crate::trigger::readable;
use crate::trigger::store::{Trigger, TriggerStore, TriggerTarget};
use crate::ScriptCall;

/// Which dispatch lane the engine is running. Mirrors
/// [`TriggerTarget`]: a `Line` pass only fires triggers with
/// `target=Line`; a `Prompt` pass only fires triggers with
/// `target=Prompt`. Lets the session loop reuse the same engine
/// for both completed lines and partial-prompt buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchScope {
    Line,
    Prompt,
    /// A line a room look lists after its exits line, one of the armies,
    /// things or people in the room. It is a completed line too, so `Line`
    /// triggers fire on it alongside the `Room` ones, in one pass, and
    /// their priorities and overlapping spans resolve together.
    Room,
    /// The line of the person you target, among the people a room look
    /// lists. It is a room line too, so `Line`, `Room` and `RoomTarget`
    /// triggers all fire on it in one pass.
    RoomTarget,
}

impl MatchScope {
    fn matches(self, target: TriggerTarget) -> bool {
        use MatchScope as S;
        use TriggerTarget as T;
        matches!(
            (self, target),
            (S::Line | S::Room | S::RoomTarget, T::Line)
                | (S::Prompt, T::Prompt)
                | (S::Room | S::RoomTarget, T::Room)
                | (S::RoomTarget, T::RoomTarget)
        )
    }
}

/// What the engine produced for one line of MUD output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineResult {
    /// Text to display. `None` means the line was gagged.
    pub display: Option<String>,
    /// Commands to send to the server, in priority order.
    pub sends: Vec<String>,
    /// Pane names to route the line to, each once, in the order the
    /// first trigger routing there fired.
    pub routes: Vec<String>,
    /// Lua script bodies queued by `TriggerAction::Script` actions,
    /// each paired with the positional regex captures from the
    /// matching pattern (group 0 is the whole match; `[1..]` are the
    /// numbered groups). The session loop evaluates these against
    /// its shared `ScriptEngine` after the line is displayed.
    pub scripts: Vec<ScriptCall>,
    /// The alert of each trigger that matched, once for each trigger, in
    /// priority order. A gagged line rings too.
    pub alerts: Vec<TriggerAlert>,
}

/// The alert of a trigger that matched a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerAlert {
    /// The trigger's name, which titles the banner.
    pub trigger: String,
    pub parts: AlertParts,
}

/// Run the trigger store against a single line of MUD output.
///
/// `original` is the raw bytes of the line as the server sent it, including
/// any embedded ANSI escapes. The trigger engine matches against the plain
/// text (escapes stripped). A highlight draws its style over the text it
/// matches and leaves the rest of the line as the game sent it, colors
/// included. A Replace or a wash rebuilds the line from its plain text.
///
/// Equivalent to `process_scoped(store, original, MatchScope::Line, key)`.
/// Test only, like `process_scoped`.
#[cfg(any(test, feature = "testkit"))]
pub fn process(store: &TriggerStore, original: &[u8], key: StopKey) -> LineResult {
    process_scoped(store, original, MatchScope::Line, key)
}

/// Run the trigger store against a buffer, only firing triggers whose
/// `target` matches the given scope, with no ground to lift colors on.
/// Only tests call it, the app's tests through the `testkit` feature.
/// The session calls [`process_on_ground`] with the plain text it
/// already holds and the terminal background.
#[cfg(any(test, feature = "testkit"))]
pub fn process_scoped(
    store: &TriggerStore,
    original: &[u8],
    scope: MatchScope,
    key: StopKey,
) -> LineResult {
    process_on_ground(
        store,
        original,
        &plain_text(original),
        scope,
        None,
        None,
        key,
    )
}

/// Run the trigger store against a line or a prompt buffer, firing only
/// the triggers whose `target` the scope takes. `plain` is `original`
/// with its escapes stripped. The session already strips it for the tick
/// reset check, so taking it here saves a second ANSI pass on every
/// output line. With `ground`, the terminal background, every fixed
/// color the triggers paint text or an underline in, a true color or a
/// 256 color past the 16, holds [`readable::READABLE_CONTRAST`] on what
/// it draws on (see [`readable::lift_sgr`]). `None` leaves each color as
/// the trigger set it. That lift never reaches the game's own colors. A
/// line no trigger matched keeps the bytes the game sent, and a highlight
/// drawn over those bytes lifts its own open alone.
///
/// With `game_ground`, the same background, the game's own 256 colors
/// past the 16 read at [`readable::GAME_LC`] on a light ground (see
/// [`readable::lift_game_sgr`]), whether no trigger runs, none matches,
/// or a highlight draws over them in place. A line a trigger rebuilt
/// from its plain text keeps none of the game's codes, and the colors
/// its triggers paint answer to `ground` alone. `None` leaves the game's
/// colors as sent.
///
/// A trigger Vosh stopped under `key`, the session the line came to,
/// matches nothing.
pub fn process_on_ground(
    store: &TriggerStore,
    original: &[u8],
    plain: &str,
    scope: MatchScope,
    ground: Option<readable::Rgb>,
    game_ground: Option<readable::Rgb>,
    key: StopKey,
) -> LineResult {
    // The game's bytes with its faded 256 colors lifted, which every path
    // below draws from in place of the bytes as sent. Only SGR parameters
    // change, so `plain` still spells them.
    let lifted = game_ground.map_or(Cow::Borrowed(original), |game| {
        readable::lift_game_sgr(original, game)
    });
    let original: &[u8] = &lifted;
    if store.is_empty() {
        return LineResult {
            display: Some(bytes_to_string_lossy(original)),
            ..Default::default()
        };
    }

    let mut gagged = false;
    let mut text = plain.to_string();
    let mut highlights: Vec<(Regex, HighlightStyle)> = Vec::new();
    let mut sends = Vec::new();
    let mut routes = Vec::new();
    let mut scripts: Vec<ScriptCall> = Vec::new();
    let mut alerts: Vec<TriggerAlert> = Vec::new();
    let mut any_match = false;
    // The SGR open of the first base style that matched, in priority
    // order. See [`HighlightStyle::base`].
    let mut base_open: Option<String> = None;
    // A Replace ran, so the line is rebuilt from its plain text.
    let mut replaced = false;

    for compiled in store.iter_compiled(key) {
        if !compiled.trigger.enabled {
            continue;
        }
        if !scope.matches(compiled.trigger.target) {
            continue;
        }
        // A trigger can hold many patterns (Mudlet-style). Each enabled
        // pattern is its own regex; we walk them all and run the
        // shared actions for every one that matches the line. This
        // keeps capture groups specific to the matching pattern, so a
        // Send template with `$1` references the correct capture
        // regardless of which sibling pattern fired.
        for regex in &compiled.regexes {
            if !regex.is_match(plain) {
                continue;
            }
            any_match = true;
            // A trigger rings once for the line, however many of its
            // patterns match.
            if let Some(parts) = &compiled.trigger.alert {
                if !alerts.iter().any(|a| a.trigger == compiled.trigger.name) {
                    alerts.push(TriggerAlert {
                        trigger: compiled.trigger.name.clone(),
                        parts: parts.clone(),
                    });
                }
            }

            // Send and Script both need this line's capture groups, and
            // they are identical, so compute them at most once per
            // (regex, line) and share. The match gate above is a plain
            // is_match, so Gag / Highlight / Route / Replace pay nothing
            // for captures they never read. Filled lazily on first use.
            let mut caps_cache: Option<Vec<regex::Captures<'_>>> = None;

            for action in &compiled.trigger.actions {
                match action {
                    TriggerAction::Gag => {
                        gagged = true;
                    }
                    TriggerAction::Replace { template } => {
                        text = regex.replace_all(&text, template.as_str()).into_owned();
                        replaced = true;
                    }
                    TriggerAction::Highlight { style } if style.base => {
                        let open = style.sgr_open();
                        if base_open.is_none() && !open.is_empty() {
                            base_open = Some(open);
                        }
                    }
                    TriggerAction::Highlight { style } => {
                        if !style.is_empty() {
                            highlights.push((regex.clone(), style.clone()));
                        }
                    }
                    TriggerAction::Send { template } => {
                        let caps_list =
                            caps_cache.get_or_insert_with(|| regex.captures_iter(plain).collect());
                        for caps in caps_list.iter() {
                            let mut buf = String::new();
                            caps.expand(template, &mut buf);
                            // Split the expanded template into one
                            // command per `;` or newline so a Send
                            // template like `get 1.;wield 1.` fires
                            // as two separate commands on the wire
                            // — matching what the user gets when
                            // typing the same line into the prompt
                            // (which the input pipeline splits via
                            // the alias engine). Without this the
                            // server sees a single line whose item
                            // name is `1.;wield 1.`.
                            for piece in split_commands(&buf) {
                                let trimmed = piece.trim();
                                if trimmed.is_empty() {
                                    continue;
                                }
                                sends.push(trimmed.to_string());
                            }
                        }
                    }
                    TriggerAction::Route { pane } => {
                        // Each pane takes the line once, however many
                        // triggers route it there.
                        if !routes.contains(pane) {
                            routes.push(pane.clone());
                        }
                    }
                    TriggerAction::Script { body } => {
                        let caps_list =
                            caps_cache.get_or_insert_with(|| regex.captures_iter(plain).collect());
                        for caps in caps_list.iter() {
                            let captures = (0..caps.len())
                                .map(|i| {
                                    caps.get(i)
                                        .map(|m| m.as_str().to_string())
                                        .unwrap_or_default()
                                })
                                .collect();
                            scripts.push(ScriptCall {
                                source: compiled.trigger.name.clone(),
                                body: body.clone(),
                                captures,
                            });
                        }
                    }
                }
            }
        }
    }

    if gagged {
        return LineResult {
            display: None,
            sends,
            routes,
            scripts,
            alerts,
        };
    }

    // Full-line wash. The first wash-flagged highlight (priority order)
    // supplies the tint: the line's text gets the wash color's quarter-
    // strength truecolor background. The native renderer recognizes
    // that exact tint on a row's first cell and paints the field across
    // the whole row, so the wash bytes are the entire contract, with no
    // side channel, and the field survives resize, reflow, and
    // scrollback reload wherever the bytes do.
    let wash_style = highlights.iter().find(|(_, s)| s.wash).map(|(_, s)| s);
    let wash_bg = wash_style.map(|s| s.wash_source().wash_tint());
    // A washed line carries the mark color in its text too, not just in
    // the field behind it. This goes out as a palette SGR code rather
    // than truecolor, so both renderers resolve it through the active
    // theme instead of pinning it to the canonical xterm chart.
    let wash_fg = wash_style
        .and_then(|s| s.fg)
        .map(crate::trigger::color::NamedColor::fg_code);
    // The attributes every washed row opens with, and that each
    // highlight span restores when it closes.
    let wash_open = wash_bg.map(|(r, g, b)| match wash_fg {
        Some(fg) => format!("\x1b[{fg};48;2;{r};{g};{b}m"),
        None => format!("\x1b[48;2;{r};{g};{b}m"),
    });

    let display = if !any_match {
        bytes_to_string_lossy(original)
    } else if !replaced && wash_open.is_none() {
        // Each highlight draws over the text it matched, and the rest of
        // the line keeps the codes the game sent. A line whose bytes do
        // not spell `plain` is rebuilt from it instead.
        draw_in_place(original, plain, highlight_spans(plain, &highlights), ground)
    } else {
        // Apply highlights last on the (possibly replaced) text so colors
        // wrap whatever the user ends up seeing.
        text = apply_highlights(&text, &highlights, wash_open.as_deref());
        if let Some(open) = &wash_open {
            // The tint covers the text only. Erasing the row to the wash
            // color would fill its blank cells too, and a narrower
            // terminal wraps those onto a tinted row of their own. The
            // native renderer extends the field to the full width. The
            // final reset keeps the following line clean.
            text = format!("{open}{text}\x1b[0m");
        }
        // The rebuild started from the plain text, so every escape in the
        // line now came from a trigger.
        if let Some(ground) = ground {
            if let Cow::Owned(lifted) = readable::lift_sgr(&text, ground) {
                text = lifted;
            }
        }
        text
    };
    // The base color fills what the line left in the default color,
    // around the game's codes and the spans above alike. Its open reads
    // on the ground the way a span's does.
    if let (Some(open), Some(ground)) = (&mut base_open, ground) {
        lift_open(open, ground);
    }
    let display = Some(match &base_open {
        Some(open) => with_base(&display, open),
        None => display,
    });

    LineResult {
        display,
        sends,
        routes,
        scripts,
        alerts,
    }
}

/// The triggers that would fire on `plain` in `scope`, in priority order,
/// without running any of their actions. A trigger counts when it is on,
/// its group is on, Vosh has not stopped it under `key`, and any of its
/// enabled patterns matches. The session uses it to name triggers, such
/// as a Line trigger that matched a line Vosh read as your prompt.
pub fn matching<'a>(
    store: &'a TriggerStore,
    plain: &str,
    scope: MatchScope,
    key: StopKey,
) -> Vec<&'a Trigger> {
    store
        .iter_compiled(key)
        .filter(|c| c.trigger.enabled && scope.matches(c.trigger.target))
        .filter(|c| c.regexes.iter().any(|r| r.is_match(plain)))
        .map(|c| &c.trigger)
        .collect()
}

/// Apply every highlight in a single pass. Match spans are collected
/// against the text BEFORE any escapes are injected, so a later
/// pattern can never match inside an earlier highlight's escape
/// sequence (a digit pattern like `\d+` used to match the digits of an
/// injected SGR and corrupt the line). Overlapping spans resolve
/// first-wins in trigger priority order. With a wash active, each
/// span's closing reset re-opens the wash attributes (field tint plus
/// the line's mark color) so both hold past the highlighted text.
fn apply_highlights(
    text: &str,
    highlights: &[(Regex, HighlightStyle)],
    wash_open: Option<&str>,
) -> String {
    let close = match wash_open {
        Some(open) => format!("{}{open}", HighlightStyle::sgr_reset()),
        None => HighlightStyle::sgr_reset().to_string(),
    };
    paint_spans(text, &highlight_spans(text, highlights), &close)
}

/// One highlight span: where it starts and ends in the text, and the SGR
/// open of its style.
type Span = (usize, usize, String);

/// The spans `highlights` cover in `text`, in text order. Overlapping
/// spans resolve first wins in trigger priority order.
fn highlight_spans(text: &str, highlights: &[(Regex, HighlightStyle)]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (regex, style) in highlights {
        let open = style.sgr_open();
        if open.is_empty() {
            // A wash-only style colors the line, not a span.
            continue;
        }
        for mat in regex.find_iter(text) {
            if mat.end() == mat.start() {
                continue;
            }
            let overlaps = spans
                .iter()
                .any(|&(s, e, _)| mat.start() < e && s < mat.end());
            if !overlaps {
                spans.push((mat.start(), mat.end(), open.clone()));
            }
        }
    }
    spans.sort_by_key(|&(start, _, _)| start);
    spans
}

/// `text` with each span in its style, closed by `close`.
fn paint_spans(text: &str, spans: &[Span], close: &str) -> String {
    let mut out = String::with_capacity(text.len() + spans.len() * 24);
    let mut last = 0;
    for (start, end, open) in spans {
        out.push_str(&text[last..*start]);
        out.push_str(open);
        out.push_str(&text[*start..*end]);
        out.push_str(close);
        last = *end;
    }
    out.push_str(&text[last..]);
    out
}

/// `original` with each of `spans` drawn over the text it covers (see
/// [`highlight_in_place`]), or the line rebuilt from `plain` when its
/// bytes do not spell it, and `original` as sent when there is no span.
/// With `ground`, each span's open first gets the lift a rebuilt line gets
/// (see [`readable::lift_sgr`]). A span opens on the terminal ground, after
/// a reset where the game had set codes, so its open alone tells what it
/// draws on. The game's codes around and after a span never reach the
/// lift, so the game's own colors stay as sent.
fn draw_in_place(
    original: &[u8],
    plain: &str,
    mut spans: Vec<Span>,
    ground: Option<readable::Rgb>,
) -> String {
    if spans.is_empty() {
        return bytes_to_string_lossy(original);
    }
    if let Some(ground) = ground {
        for (_, _, open) in &mut spans {
            lift_open(open, ground);
        }
    }
    highlight_in_place(original, plain, &spans)
        .unwrap_or_else(|| paint_spans(plain, &spans, HighlightStyle::sgr_reset()))
}

/// `open`, an SGR sequence a trigger draws with, with each fixed color in
/// it lifted to read on `ground`.
fn lift_open(open: &mut String, ground: readable::Rgb) {
    if let Cow::Owned(lifted) = readable::lift_sgr(open, ground) {
        *open = lifted;
    }
}

/// `original` with each span, at its place in `plain`, drawn in its style
/// over the text it covers, and every byte outside the spans as the game
/// sent it. The game's own SGR codes inside a span drop, so the span shows
/// in its style alone, and after it the line goes on in the colors the
/// game had set by then. A span on text the game left uncolored opens and
/// closes exactly as on a plain line. None when the bytes do not spell
/// `plain`, so the caller rebuilds the line from its plain text.
fn highlight_in_place(original: &[u8], plain: &str, spans: &[Span]) -> Option<String> {
    let pieces = vosh_protocol::ansi::pieces(original);
    let mut out = String::with_capacity(original.len() + spans.len() * 24);
    // The game's SGR codes in effect since its last reset.
    let mut state: Vec<String> = Vec::new();
    // The bytes of `original` before this are in `out` or dropped.
    let mut copied = 0;
    // Where the walk is in `plain`.
    let mut at = 0;
    // The end of the span drawing now.
    let mut open: Option<usize> = None;
    let mut spans = spans.iter().peekable();
    for (i, piece) in pieces.iter().enumerate() {
        match &piece.kind {
            PieceKind::Text(c) => {
                if !plain.get(at..)?.starts_with(*c) {
                    return None;
                }
                if open.is_none() {
                    if let Some((_, end, style)) = spans.next_if(|(start, ..)| *start == at) {
                        out.push_str(&String::from_utf8_lossy(&original[copied..piece.raw.start]));
                        if state.is_empty() {
                            out.push_str(style);
                        } else {
                            // The game's attributes stop at the span.
                            out.push_str("\x1b[0;");
                            out.push_str(style.strip_prefix("\x1b[").unwrap_or(style));
                        }
                        open = Some(*end);
                    }
                }
                at += c.len_utf8();
                if let Some(end) = open {
                    out.push(*c);
                    copied = piece.raw.end;
                    if end == at {
                        open = None;
                        // A reset the game sends next ends the span on
                        // its own.
                        let next_resets = matches!(
                            pieces.get(i + 1).map(|p| &p.kind),
                            Some(PieceKind::Sgr(params)) if resets(params)
                        );
                        if !next_resets {
                            out.push_str(&restore(&state));
                        }
                    }
                }
            }
            PieceKind::Sgr(params) => {
                fold_sgr(&mut state, params);
                if open.is_some() {
                    copied = piece.raw.end;
                }
            }
            PieceKind::Other => {
                if open.is_some() {
                    out.push_str(&String::from_utf8_lossy(&original[piece.raw.clone()]));
                    copied = piece.raw.end;
                }
            }
        }
    }
    if at != plain.len() {
        return None;
    }
    out.push_str(&String::from_utf8_lossy(&original[copied..]));
    Some(out)
}

/// Whether the SGR `params` open with a reset, `0` or no number at all.
fn resets(params: &str) -> bool {
    let first = params.split([';', ':']).next().unwrap_or("");
    first.is_empty() || first.parse::<u32>() == Ok(0)
}

/// Fold the SGR `params` into `state`, the codes in effect since the
/// last reset. A reset clears it, and every other code joins it, an
/// extended color with its arguments, so the `0` in `48;5;0` never reads
/// as a reset.
fn fold_sgr(state: &mut Vec<String>, params: &str) {
    let codes: Vec<&str> = params.split(';').collect();
    let mut i = 0;
    while i < codes.len() {
        let code = codes[i];
        let (head, inline) = match code.split_once(':') {
            Some((head, _)) => (head, true),
            None => (code, false),
        };
        if head.is_empty() || head.parse::<u32>() == Ok(0) {
            state.clear();
            i += 1;
            continue;
        }
        let width = match (head, inline, codes.get(i + 1)) {
            ("38" | "48" | "58", false, Some(&"5")) => 3,
            ("38" | "48" | "58", false, Some(&"2")) => 5,
            _ => 1,
        };
        let end = (i + width).min(codes.len());
        state.extend(codes[i..end].iter().map(|c| (*c).to_string()));
        i = end;
    }
}

/// The SGR sequence that ends a span and puts back `state`, the game's
/// codes in effect there.
fn restore(state: &[String]) -> String {
    if state.is_empty() {
        HighlightStyle::sgr_reset().to_string()
    } else {
        format!("\x1b[0;{}m", state.join(";"))
    }
}

/// `text` with `open`, a base color, at its start and again after each
/// SGR sequence that leaves the foreground at its default, then a reset at
/// its end. A sequence that sets a foreground of its own, as the game's
/// color codes and a highlight span's open do, stays as it is.
fn with_base(text: &str, open: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + open.len() * 2 + 4);
    out.push_str(open);
    let mut copied = 0;
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] != 0x1b || bytes[at + 1] != b'[' {
            at += 1;
            continue;
        }
        // A CSI sequence runs to its final byte, 0x40 to 0x7e.
        let Some(len) = bytes[at + 2..]
            .iter()
            .position(|b| (0x40..=0x7e).contains(b))
        else {
            break;
        };
        let last = at + 2 + len;
        out.push_str(&text[copied..=last]);
        copied = last + 1;
        if bytes[last] == b'm' && leaves_default_fg(&text[at + 2..last]) {
            out.push_str(open);
        }
        at = copied;
    }
    out.push_str(&text[copied..]);
    out.push_str(HighlightStyle::sgr_reset());
    out
}

/// Whether an SGR sequence with `params` leaves the foreground at its
/// default: its last code that touches the foreground is a reset (`0` or
/// none at all) or `39`. Extended colors skip their own arguments, so
/// the `5` in `48;5;0` never reads as a code.
fn leaves_default_fg(params: &str) -> bool {
    let mut default = false;
    let mut codes = params.split(';');
    while let Some(code) = codes.next() {
        // A colon form like `38:5:208` carries its arguments with it.
        let (head, inline) = match code.split_once(':') {
            Some((head, _)) => (head, true),
            None => (code, false),
        };
        let n = if head.is_empty() {
            0
        } else {
            match head.parse::<u32>() {
                Ok(n) => n,
                Err(_) => continue,
            }
        };
        match n {
            0 | 39 => default = true,
            30..=37 | 90..=97 => default = false,
            38 | 48 | 58 => {
                if n == 38 {
                    default = false;
                }
                if !inline {
                    match codes.next() {
                        Some("5") => {
                            codes.next();
                        }
                        Some("2") => {
                            codes.nth(2);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    default
}

fn bytes_to_string_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::action::HighlightStyle;
    use crate::trigger::color::NamedColor;

    /// The session every line here comes to.
    const SESSION: StopKey = StopKey(1);

    fn store(triggers: Vec<Trigger>) -> TriggerStore {
        let mut s = TriggerStore::new();
        for t in triggers {
            s.set(t).unwrap();
        }
        s
    }

    fn highlight(name: &str, pattern: &str, fg: NamedColor) -> Trigger {
        Trigger::new(
            name,
            pattern,
            TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(fg),
                    ..Default::default()
                },
            },
        )
    }

    #[test]
    fn import_json_preserves_disabled_groups() {
        // The Settings editor saves through a full import; the
        // disabled-groups set is user state about GROUPS and must
        // survive the store replacement, or every save silently
        // re-enables all disabled groups (and an individually enabled
        // trigger in a "disabled" group starts firing again).
        let mut t = highlight("t1", "^ouch", NamedColor::Red);
        t.group = Some("combat".to_string());
        let mut s = store(vec![t]);
        s.set_disabled_groups(vec!["combat".to_string()]);
        let json = s.export_json().unwrap();
        s.import_json(&json).unwrap();
        assert_eq!(s.disabled_groups(), vec!["combat".to_string()]);
        // And the gate holds: the trigger in the disabled group is
        // filtered even though its own enabled flag is true.
        let r = process(&s, b"ouch that hurt", SESSION);
        assert!(r.display.is_some());
        assert_eq!(r.display.as_deref(), Some("ouch that hurt"));
    }

    #[test]
    fn import_json_error_path_leaves_store_untouched() {
        // One invalid pattern among valid triggers must reject the
        // import wholesale: items AND the disabled-groups set stay
        // exactly as they were. Pins the take-after-success ordering
        // in import_json; hoisting the take back above the build loop
        // would silently empty disabled_groups on this path.
        let mut t = highlight("t1", "^ouch", NamedColor::Red);
        t.group = Some("combat".to_string());
        let mut s = store(vec![t]);
        s.set_disabled_groups(vec!["combat".to_string()]);
        let bad_json = s.export_json().unwrap().replace("^ouch", "([");
        assert!(s.import_json(&bad_json).is_err());
        assert_eq!(s.len(), 1);
        assert_eq!(s.disabled_groups(), vec!["combat".to_string()]);
    }

    #[test]
    fn matching_names_the_triggers_a_line_would_fire_in_its_scope() {
        let mut prompt = highlight("prompt-look", "hp", NamedColor::Blue);
        prompt.target = TriggerTarget::Prompt;
        let mut off = highlight("off", "hp", NamedColor::Red);
        off.enabled = false;
        let mut grouped = highlight("grouped", "hp", NamedColor::Red);
        grouped.group = Some("combat".to_string());
        let mut two = highlight("two", "^nothing", NamedColor::Red);
        two.patterns
            .push(crate::trigger::store::TriggerPattern::regex(r"\d+hp"));
        let mut s = store(vec![
            highlight("low", "hp", NamedColor::Red),
            prompt,
            off,
            grouped,
            two,
            highlight("other", "^You", NamedColor::Red),
        ]);
        s.set_disabled_groups(vec!["combat".to_string()]);

        let names = |scope| -> Vec<String> {
            matching(&s, "[850/900hp]", scope, SESSION)
                .into_iter()
                .map(|t| t.name.clone())
                .collect()
        };
        // A turned off trigger, one in a turned off group and one whose
        // patterns all miss are left out. Any pattern that hits counts.
        let mut line = names(MatchScope::Line);
        line.sort();
        assert_eq!(line, ["low", "two"]);
        assert_eq!(names(MatchScope::Prompt), ["prompt-look"]);
        let empty = TriggerStore::new();
        let leftover = matching(&empty, "hp", MatchScope::Line, SESSION);
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn no_triggers_returns_original() {
        let s = TriggerStore::new();
        let r = process(&s, b"plain text", SESSION);
        assert_eq!(r.display.as_deref(), Some("plain text"));
        let leftover = &r.sends;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn no_match_returns_original() {
        let s = store(vec![highlight("h", "goblin", NamedColor::Cyan)]);
        let r = process(&s, b"a peaceful meadow", SESSION);
        assert_eq!(r.display.as_deref(), Some("a peaceful meadow"));
    }

    #[test]
    fn highlight_wraps_matched_substring() {
        let s = store(vec![highlight("tells", r"\w+ tells you", NamedColor::Cyan)]);
        let r = process(&s, b"Bob tells you 'hi'", SESSION);
        let text = r.display.unwrap();
        assert!(text.contains("\x1b[36m"));
        assert!(text.contains("Bob tells you"));
        assert!(text.contains("\x1b[0m"));
    }

    /// A trigger on `$n walks in.`, from `act_move.c:1053`, that rings `parts`.
    fn visitor(parts: crate::alert::AlertParts) -> Trigger {
        Trigger {
            alert: Some(parts),
            ..Trigger::new("visitor", r"^(\w+) walks in\.$", TriggerAction::Gag)
        }
    }

    #[test]
    fn a_trigger_rings_its_alert_once_for_a_line_even_when_it_hides_it() {
        let parts = crate::alert::AlertParts {
            banner: true,
            ..Default::default()
        };
        let mut trigger = visitor(parts.clone());
        trigger
            .patterns
            .push(crate::trigger::TriggerPattern::regex("walks in"));
        let s = store(vec![
            trigger,
            highlight("quiet", "walks", NamedColor::Yellow),
        ]);
        let r = process(&s, b"Maren walks in.", SESSION);
        assert!(r.display.is_none(), "the visitor trigger gags the line");
        assert_eq!(
            r.alerts,
            [TriggerAlert {
                trigger: "visitor".into(),
                parts,
            }]
        );
        let r = process(&s, b"Maren leaves south.", SESSION);
        assert!(r.alerts.is_empty(), "{:?}", r.alerts);
    }

    #[test]
    fn a_trigger_vosh_stopped_or_turned_off_rings_nothing() {
        let mut s = store(vec![visitor(crate::alert::AlertParts::default())]);
        s.stop("visitor", SESSION);
        let rung = process(&s, b"Maren walks in.", SESSION).alerts;
        assert!(rung.is_empty(), "{rung:?}");
        assert_eq!(process(&s, b"Maren walks in.", StopKey(2)).alerts.len(), 1);
        let off = Trigger {
            enabled: false,
            ..visitor(crate::alert::AlertParts::default())
        };
        let s = store(vec![off]);
        let rung = process(&s, b"Maren walks in.", SESSION).alerts;
        assert!(rung.is_empty(), "{rung:?}");
    }

    #[test]
    fn gag_drops_line() {
        let s = store(vec![Trigger::new("spam", "tingle", TriggerAction::Gag)]);
        let r = process(&s, b"You feel a tingle.", SESSION);
        assert!(r.display.is_none());
    }

    #[test]
    fn replace_substitutes_with_capture() {
        let s = store(vec![Trigger::new(
            "rename",
            r"goblin",
            TriggerAction::Replace {
                template: "wolf".into(),
            },
        )]);
        let r = process(&s, b"You see a goblin.", SESSION);
        assert_eq!(r.display.as_deref(), Some("You see a wolf."));
    }

    #[test]
    fn replace_uses_named_capture() {
        let s = store(vec![Trigger::new(
            "polite",
            r"(?<who>\w+) yells",
            TriggerAction::Replace {
                template: "$who calmly says".into(),
            },
        )]);
        let r = process(&s, b"Bob yells", SESSION);
        assert_eq!(r.display.as_deref(), Some("Bob calmly says"));
    }

    #[test]
    fn send_substitutes_capture() {
        let s = store(vec![Trigger::new(
            "loot",
            r"The (\w+) is DEAD",
            TriggerAction::Send {
                template: "loot $1".into(),
            },
        )]);
        let r = process(&s, b"The goblin is DEAD!", SESSION);
        assert_eq!(r.sends, vec!["loot goblin".to_string()]);
    }

    #[test]
    fn route_appends_pane_name() {
        let s = store(vec![Trigger::new(
            "tells",
            r"tells you",
            TriggerAction::Route {
                pane: "chat".into(),
            },
        )]);
        let r = process(&s, b"Bob tells you 'hi'", SESSION);
        assert_eq!(r.routes, vec!["chat".to_string()]);
    }

    #[test]
    fn route_sends_a_line_to_each_pane_once() {
        // A trigger you built and a preset that route the same line to
        // the same pane show it there once, not twice.
        let route = |name: &str, pane: &str, priority: i32| Trigger {
            priority,
            ..Trigger::new(
                name,
                r"^You tell ",
                TriggerAction::Route { pane: pane.into() },
            )
        };
        let s = store(vec![
            route("mine", "tell", 2),
            route("log", "chat", 1),
            route("preset", "tell", 0),
        ]);
        let r = process(&s, b"You tell Tolliver 'hi'", SESSION);
        assert_eq!(r.routes, vec!["tell".to_string(), "chat".to_string()]);
    }

    #[test]
    fn priority_order_is_high_to_low() {
        let mut s = TriggerStore::new();
        s.set(Trigger {
            priority: -10,
            ..Trigger::new(
                "low",
                "x",
                TriggerAction::Replace {
                    template: "L".into(),
                },
            )
        })
        .unwrap();
        s.set(Trigger {
            priority: 100,
            ..Trigger::new(
                "high",
                "x",
                TriggerAction::Replace {
                    template: "H".into(),
                },
            )
        })
        .unwrap();
        // High runs first on plain text "x". After high replaces to "H",
        // low matches against original plain text "x" (no match in
        // resulting "H"), so its replace also runs on the original "x"
        // pattern against current text "H" which finds nothing. Net result
        // is "H".
        let r = process(&s, b"x", SESSION);
        assert_eq!(r.display.as_deref(), Some("H"));
    }

    #[test]
    fn disabled_trigger_does_not_fire() {
        let mut s = TriggerStore::new();
        let mut t = highlight("tells", r"tells you", NamedColor::Cyan);
        t.enabled = false;
        s.set(t).unwrap();
        let r = process(&s, b"Bob tells you 'hi'", SESSION);
        let text = r.display.unwrap();
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn ansi_in_input_is_stripped_for_match() {
        let s = store(vec![highlight("tells", r"Bob tells you", NamedColor::Cyan)]);
        // Server sends gray text. Trigger should still match, and the
        // rest of the line stays gray.
        let r = process(&s, b"\x1b[37mBob tells you 'hi'\x1b[0m", SESSION);
        assert_eq!(
            r.display.as_deref(),
            Some("\x1b[37m\x1b[0;36mBob tells you\x1b[0;37m 'hi'\x1b[0m")
        );
    }

    /// The `WiZNET` line `act_wiz.c` sends for `message`, the tag in `` `& ``
    /// bold white and `` `8 `` grey, the time, and the message as its caller
    /// wrote it.
    fn wiznet(message: &str) -> String {
        format!(
            "\x1b[0;1;37mW\x1b[0;1;30mi\x1b[0;1;37mZNET\x1b[0;1;30m \x1b[0;0m08:20:01\
             \x1b[0;1;30m: \x1b[0;0m{message}"
        )
    }

    /// The tag of a `WiZNET` line in `open`, and the rest as the game sent
    /// it.
    fn wiznet_tagged(open: &str, message: &str) -> String {
        format!(
            "\x1b[0;1;37m{open}WiZNET\x1b[0;1;30m \x1b[0;0m08:20:01\x1b[0;1;30m: \
             \x1b[0;0m{message}"
        )
    }

    #[test]
    fn a_highlight_keeps_the_colors_the_game_put_on_the_rest_of_the_line() {
        let mut tag = highlight("wiznet.tag", r"^WiZNET\b", NamedColor::Magenta);
        tag.actions = vec![TriggerAction::Highlight {
            style: HighlightStyle {
                fg: Some(NamedColor::Magenta),
                bold: true,
                ..Default::default()
            },
        }];
        let s = store(vec![tag]);
        // The game's grey i inside the tag drops, so the tag is magenta
        // whole. Its reset right after the tag ends the span, and the
        // message keeps its color: comm.c in `! bold red, magic.c in `&
        // bold white, and update.c in `@ bold green.
        for message in [
            "\x1b[0;1;31mCorrupted Pfile detected: Tolliver\x1b[0;0m",
            "\x1b[0;1;37mTolliver attacked Maren at 5279\x1b[0;0m",
            "\x1b[0;1;32mTolliver has been forced wizinvis for idling > 13 ticks.\x1b[0;0m",
            "TICK!",
        ] {
            assert_eq!(
                process(&s, wiznet(message).as_bytes(), SESSION).display,
                Some(wiznet_tagged("\x1b[0;1;35m", message)),
                "{message}"
            );
        }
    }

    #[test]
    fn a_span_on_uncolored_text_opens_and_closes_as_on_a_plain_line() {
        // char_to_char for a resting player who is away, the AFK in `1
        // red. After the game's reset the name is uncolored, so the span
        // opens and closes as it would on a plain line.
        let s = store(vec![highlight("name", "Tolliver", NamedColor::Cyan)]);
        let line = "[\x1b[0;31mAFK\x1b[0;0m] Tolliver is resting here.";
        assert_eq!(
            process(&s, line.as_bytes(), SESSION).display.as_deref(),
            Some("[\x1b[0;31mAFK\x1b[0;0m] \x1b[36mTolliver\x1b[0m is resting here.")
        );
    }

    #[test]
    fn the_game_color_comes_back_after_a_span_inside_it() {
        // A say in `# bold yellow, quoting a time of day message.
        let s = store(vec![highlight("day", "day", NamedColor::Cyan)]);
        let line = "Tolliver says '\x1b[0;1;33mThe day has begun.\x1b[0;0m'";
        assert_eq!(
            process(&s, line.as_bytes(), SESSION).display.as_deref(),
            Some(
                "Tolliver says '\x1b[0;1;33mThe \x1b[0;36mday\x1b[0;1;33m has begun.\
                 \x1b[0;0m'"
            )
        );
    }

    #[test]
    fn the_game_codes_fold_into_what_a_span_puts_back() {
        let folded = |sequences: &[&str]| {
            let mut state = Vec::new();
            for params in sequences {
                fold_sgr(&mut state, params);
            }
            restore(&state)
        };
        assert_eq!(folded(&[]), "\x1b[0m");
        assert_eq!(folded(&["0;1;31", "0;0"]), "\x1b[0m");
        assert_eq!(folded(&["0;1;31"]), "\x1b[0;1;31m");
        assert_eq!(folded(&["1", "4", "0;33"]), "\x1b[0;33m");
        // The 0 in an extended color is an argument, never a reset.
        assert_eq!(
            folded(&["38;5;208;48;5;0", "1"]),
            "\x1b[0;38;5;208;48;5;0;1m"
        );
        assert_eq!(folded(&["48;2;0;0;0", "4:3"]), "\x1b[0;48;2;0;0;0;4:3m");
        assert!(resets("0;1;30"));
        assert!(resets("0"));
        assert!(!resets("1;30"));
        assert!(!resets("38;5;0"));
    }

    #[test]
    fn a_trigger_that_draws_nothing_keeps_the_line_as_sent() {
        let s = store(vec![Trigger {
            name: "away".into(),
            patterns: vec![crate::trigger::store::TriggerPattern::regex(
                r"^\[AFK\] (\w+) is resting here\.$",
            )],
            priority: 0,
            enabled: true,
            actions: vec![
                TriggerAction::Send {
                    template: "wake $1".into(),
                },
                TriggerAction::Route {
                    pane: "group".into(),
                },
            ],
            preset: None,
            group: None,
            target: TriggerTarget::Line,
            alert: None,
        }]);
        let line = "[\x1b[0;31mAFK\x1b[0;0m] Tolliver is resting here.";
        let r = process(&s, line.as_bytes(), SESSION);
        assert_eq!(r.display.as_deref(), Some(line));
        assert_eq!(r.sends, ["wake Tolliver"]);
    }

    #[test]
    fn a_replace_or_a_wash_still_rebuilds_the_line_from_its_text() {
        let line = b"Tolliver says '\x1b[0;1;33mThe day has begun.\x1b[0;0m'";
        let mut rename = highlight("rename", "Tolliver", NamedColor::Cyan);
        rename.actions = vec![TriggerAction::Replace {
            template: "Maren".into(),
        }];
        assert_eq!(
            process(&store(vec![rename]), line, SESSION)
                .display
                .as_deref(),
            Some("Maren says 'The day has begun.'")
        );
        let mut wash = highlight("wash", "says", NamedColor::Red);
        wash.actions = vec![TriggerAction::Highlight {
            style: HighlightStyle {
                fg: Some(NamedColor::Red),
                wash: true,
                ..Default::default()
            },
        }];
        let washed = process(&store(vec![wash]), line, SESSION).display.unwrap();
        assert!(!washed.contains("\x1b[0;1;33m"), "{washed:?}");
    }

    #[test]
    fn a_line_whose_bytes_do_not_spell_its_plain_text_is_rebuilt() {
        let s = store(vec![highlight("name", "Tolliver", NamedColor::Cyan)]);
        let r = process_on_ground(
            &s,
            b"\x1b[0;32mMaren\x1b[0;0m",
            "Tolliver",
            MatchScope::Line,
            None,
            None,
            SESSION,
        );
        assert_eq!(r.display.as_deref(), Some("\x1b[36mTolliver\x1b[0m"));
    }

    #[test]
    fn a_base_color_fills_around_a_span_kept_in_place() {
        let mut name = highlight("name", "Tolliver", NamedColor::Cyan);
        name.priority = 5;
        let s = store(vec![base("room", "^.+$", NamedColor::Yellow, 4), name]);
        let line = "[\x1b[0;31mAFK\x1b[0;0m] Tolliver is resting here.";
        assert_eq!(
            process(&s, line.as_bytes(), SESSION).display.as_deref(),
            Some(
                "\x1b[33m[\x1b[0;31mAFK\x1b[0;0m\x1b[33m] \x1b[36mTolliver\x1b[0m\x1b[33m \
                 is resting here.\x1b[0m"
            )
        );
    }

    #[test]
    fn a_highlight_in_place_never_changes_the_text_a_line_shows() {
        // Every line of the room colors fixtures, game text with the codes
        // the server sends, with each of its words highlighted in turn.
        let mut lines: Vec<String> = Vec::new();
        for text in [
            include_str!("../../../../fixtures/room-colors/lines.json"),
            include_str!("../../../../fixtures/room-colors/looks.json"),
        ] {
            let json: serde_json::Value = serde_json::from_str(text).unwrap();
            let mut stack = vec![json];
            while let Some(value) = stack.pop() {
                match value {
                    serde_json::Value::Object(map) => {
                        if let Some(serde_json::Value::String(line)) = map.get("line") {
                            lines.push(line.clone());
                        }
                        stack.extend(map.into_iter().map(|(_, v)| v));
                    }
                    serde_json::Value::Array(list) => stack.extend(list),
                    _ => {}
                }
            }
        }
        assert!(lines.len() > 40, "{}", lines.len());
        let word = Regex::new(r"\w+").unwrap();
        let cyan = HighlightStyle {
            fg: Some(NamedColor::Cyan),
            ..Default::default()
        };
        let mut drawn = 0;
        for line in &lines {
            let plain = plain_text(line.as_bytes());
            for found in word.find_iter(&plain) {
                let pattern = Regex::new(&format!(r"\b{}\b", regex::escape(found.as_str())))
                    .expect("a word pattern compiles");
                let spans = highlight_spans(&plain, &[(pattern, cyan.clone())]);
                let shown = highlight_in_place(line.as_bytes(), &plain, &spans)
                    .unwrap_or_else(|| panic!("{line:?} draws in place"));
                assert_eq!(plain_text(shown.as_bytes()), plain, "{line:?} {found:?}");
                assert!(shown.contains("36m"), "{line:?} {found:?}");
                drawn += 1;
            }
        }
        assert!(drawn > 300, "{drawn}");
    }

    /// The plain text of each line of fixtures/room-colors/looks.json that
    /// holds a word, in file order.
    fn look_lines() -> Vec<String> {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../../../fixtures/room-colors/looks.json"))
                .unwrap();
        let mut out = Vec::new();
        for case in json["cases"].as_array().unwrap() {
            for event in case["events"].as_array().unwrap() {
                if let Some(line) = event["line"].as_str() {
                    let plain = plain_text(line.as_bytes());
                    if plain.chars().any(char::is_alphanumeric) {
                        out.push(plain);
                    }
                }
            }
        }
        out
    }

    /// A trigger with one row in `mode`, highlighting cyan.
    fn in_mode(pattern: &str, mode: crate::trigger::MatchMode) -> Trigger {
        let mut t = highlight("mode", "", NamedColor::Cyan);
        t.patterns[0] = crate::trigger::TriggerPattern {
            mode,
            ..crate::trigger::TriggerPattern::regex(pattern)
        };
        t
    }

    #[test]
    fn a_line_copied_with_or_without_its_spaces_matches_in_text_and_starts_with() {
        use crate::trigger::MatchMode;
        let lines = look_lines();
        // Things in a look print after five spaces.
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("     A black-steel helm")),
            "{lines:?}"
        );
        for line in &lines {
            // The first dozen letters of the line, as you might copy them.
            let text = line.trim_start();
            let start = &text[..text.char_indices().nth(12).map_or(text.len(), |(i, _)| i)];
            for (copy, mode) in [
                (line.as_str(), MatchMode::Text),
                (line.trim_start(), MatchMode::Text),
                (line.trim(), MatchMode::Text),
                (line.as_str(), MatchMode::StartsWith),
                (line.trim_start(), MatchMode::StartsWith),
                (start, MatchMode::StartsWith),
            ] {
                let s = store(vec![in_mode(copy, mode)]);
                assert_eq!(
                    matching(&s, line, MatchScope::Line, SESSION).len(),
                    1,
                    "{mode:?} {copy:?} on {line:?}"
                );
                // The span covers the whole line, the spaces included.
                let compiled = s.iter_compiled(SESSION).next().unwrap();
                let style = HighlightStyle {
                    fg: Some(NamedColor::Cyan),
                    ..Default::default()
                };
                let spans = highlight_spans(line, &[(compiled.regexes[0].clone(), style)]);
                assert_eq!(spans.len(), 1, "{mode:?} {copy:?}");
                assert_eq!(
                    (spans[0].0, spans[0].1),
                    (0, line.len()),
                    "{mode:?} {copy:?}"
                );
            }
        }
    }

    #[test]
    fn text_matches_only_the_whole_line_and_starts_with_only_its_start() {
        use crate::trigger::MatchMode;
        let s = store(vec![in_mode("walks in.", MatchMode::Text)]);
        let leftover = &matching(&s, "Maren walks in.", MatchScope::Line, SESSION);
        assert!(leftover.is_empty(), "{leftover:?}");
        let s = store(vec![in_mode("walks in", MatchMode::StartsWith)]);
        let leftover = &matching(&s, "Maren walks in.", MatchScope::Line, SESSION);
        assert!(leftover.is_empty(), "{leftover:?}");
        // Text skips trailing spaces on the line.
        let s = store(vec![in_mode("Maren walks in.", MatchMode::Text)]);
        assert_eq!(
            matching(&s, "Maren walks in.  ", MatchScope::Line, SESSION).len(),
            1
        );
    }

    #[test]
    fn spaces_at_the_ends_of_the_text_you_typed_never_stop_a_match() {
        use crate::trigger::MatchMode;
        // A Text pattern copied with a space after it matches the line
        // without one.
        let s = store(vec![in_mode("You feel better. ", MatchMode::Text)]);
        assert_eq!(
            matching(&s, "You feel better.", MatchScope::Line, SESSION).len(),
            1
        );
        // A pattern copied with the five spaces of a look matches the same
        // words printed with none.
        for mode in [MatchMode::Text, MatchMode::StartsWith] {
            let s = store(vec![in_mode("     Maren walks in.", mode)]);
            assert_eq!(
                matching(&s, "Maren walks in.", MatchScope::Line, SESSION).len(),
                1,
                "{mode:?}"
            );
        }
    }

    #[test]
    fn a_starts_with_highlight_colors_the_whole_line() {
        use crate::trigger::MatchMode;
        let line = "     A black-steel helm is here, gleaming darkly.";
        let s = store(vec![in_mode("A black-steel helm", MatchMode::StartsWith)]);
        assert_eq!(
            process(&s, line.as_bytes(), SESSION).display.as_deref(),
            Some(format!("\x1b[36m{line}\x1b[0m").as_str())
        );
    }

    #[test]
    fn text_and_starts_with_have_no_groups() {
        use crate::trigger::MatchMode;
        let line = "Maren walks in.";
        for mode in [MatchMode::Text, MatchMode::StartsWith] {
            let mut t = in_mode("Maren (walks) in.", mode);
            t.actions = vec![
                TriggerAction::Send {
                    template: "say [$1]".into(),
                },
                TriggerAction::Script { body: "x".into() },
            ];
            // The parentheses are text, so this pattern needs them in the
            // line.
            let s = store(vec![t.clone()]);
            let leftover = &process(&s, line.as_bytes(), SESSION).sends;
            assert!(leftover.is_empty(), "{leftover:?}");
            t.patterns[0].pattern = "Maren walks".into();
            if mode == MatchMode::Text {
                t.patterns[0].pattern = line.into();
            }
            let r = process(&store(vec![t]), line.as_bytes(), SESSION);
            assert_eq!(r.sends, ["say []"], "{mode:?}");
            assert_eq!(r.scripts[0].captures, [line], "{mode:?}");
        }
    }

    #[test]
    fn a_more_pattern_keeps_its_own_mode() {
        use crate::trigger::MatchMode;
        let mut t = in_mode("The day has begun.", MatchMode::Text);
        t.patterns.push(crate::trigger::TriggerPattern {
            mode: MatchMode::StartsWith,
            ..crate::trigger::TriggerPattern::regex("Maren")
        });
        t.patterns
            .push(crate::trigger::TriggerPattern::regex(r"^\[Exits: (\w+)\]$"));
        let s = store(vec![t]);
        for line in ["The day has begun.", "Maren walks in.", "[Exits: south]"] {
            assert_eq!(
                matching(&s, line, MatchScope::Line, SESSION).len(),
                1,
                "{line}"
            );
        }
        // The More pattern in Starts with reads Maren as text at the start,
        // so a line that holds Maren after its start stays plain.
        let leftover = &matching(
            &s,
            "Chuckling and grinning to herself, Orla walks in and quickly prepares the gallows for Maren.",
            MatchScope::Line, SESSION,
        );
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn invalid_regex_rejected_at_set() {
        let mut s = TriggerStore::new();
        let bad = Trigger::new("bad", "[unclosed", TriggerAction::Gag);
        assert!(s.set(bad).is_err());
    }

    #[test]
    fn multi_pattern_matches_either_row() {
        let s = store(vec![Trigger {
            name: "mobs".into(),
            patterns: vec![
                crate::trigger::store::TriggerPattern::regex("goblin"),
                crate::trigger::store::TriggerPattern::regex("orc"),
            ],
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Highlight {
                style: HighlightStyle {
                    fg: Some(NamedColor::Red),
                    ..Default::default()
                },
            }],
            preset: None,
            group: None,
            target: TriggerTarget::Line,
            alert: None,
        }]);
        let r1 = process(&s, b"You see a goblin.", SESSION);
        assert!(r1.display.unwrap().contains("\x1b[31m"));
        let r2 = process(&s, b"An orc charges.", SESSION);
        assert!(r2.display.unwrap().contains("\x1b[31m"));
    }

    #[test]
    fn disabled_pattern_row_is_skipped() {
        let s = store(vec![Trigger {
            name: "mobs".into(),
            patterns: vec![
                crate::trigger::store::TriggerPattern::regex("goblin"),
                crate::trigger::store::TriggerPattern {
                    enabled: false,
                    ..crate::trigger::store::TriggerPattern::regex("orc")
                },
            ],
            priority: 0,
            enabled: true,
            actions: vec![TriggerAction::Gag],
            preset: None,
            group: None,
            target: TriggerTarget::Line,
            alert: None,
        }]);
        // goblin pattern (enabled) gags.
        assert!(process(&s, b"You see a goblin.", SESSION).display.is_none());
        // orc pattern (disabled) does not fire.
        assert_eq!(
            process(&s, b"An orc charges.", SESSION).display.as_deref(),
            Some("An orc charges.")
        );
    }

    #[test]
    fn import_export_round_trip() {
        let mut s = TriggerStore::new();
        s.set(highlight("tells", r"tells you", NamedColor::Cyan))
            .unwrap();
        s.set(Trigger {
            priority: 50,
            ..Trigger::new("spam", "tingle", TriggerAction::Gag)
        })
        .unwrap();
        let json = s.export_json().unwrap();

        let mut s2 = TriggerStore::new();
        let count = s2.import_json(&json).unwrap();
        assert_eq!(count, 2);
        assert_eq!(s.list(), s2.list());
    }

    #[test]
    fn wash_wraps_whole_line() {
        let mut t = highlight("sancdown", "sanctuary", NamedColor::Yellow);
        if let TriggerAction::Highlight { style } = &mut t.actions[0] {
            style.wash = true;
        }
        let s = store(vec![t]);
        let r = process(&s, b"Your sanctuary flickers and fades.", SESSION);
        let text = r.display.unwrap();
        // Quarter-strength canonical yellow (0xcd/4 = 0x33 = 51) —
        // NamedColor::Yellow.wash_tint(), the exact value the native
        // renderer detects to paint the row's field. No row erase, so
        // a narrower terminal never wraps blank tinted cells.
        assert!(text.starts_with("\x1b[33;48;2;51;51;0mYour"));
        assert!(!text.contains("\x1b[2K"));
        assert!(text.ends_with("\x1b[0m"));
        // The matched-text close re-opens the wash attributes so both the
        // field tint and the line's mark color survive past the
        // highlighted word.
        assert!(text.contains("\x1b[0m\x1b[33;48;2;51;51;0m"));
    }

    #[test]
    fn wash_off_keeps_plain_close() {
        let s = store(vec![highlight("tells", r"tells you", NamedColor::Cyan)]);
        let r = process(&s, b"Bob tells you 'hi'", SESSION);
        let text = r.display.unwrap();
        assert!(!text.contains("48;2;"));
        assert!(text.contains("\x1b[0m"));
    }

    #[test]
    fn wash_uses_explicit_bg_over_fg() {
        let mut t = highlight("alert", "DANGER", NamedColor::White);
        if let TriggerAction::Highlight { style } = &mut t.actions[0] {
            style.wash = true;
            style.bg = Some(NamedColor::Red);
        }
        let s = store(vec![t]);
        let r = process(&s, b"DANGER close behind you", SESSION);
        let text = r.display.unwrap();
        // Wash derives from the explicit red bg (0xcd/4 = 51), not the
        // white fg.
        assert!(text.starts_with("\x1b[37;48;2;51;0;0m"));
    }

    #[test]
    fn highlight_never_matches_inside_injected_escapes() {
        // A wash plus a digit-hungry second highlight on one line. Spans
        // are collected on the clean text, so the digits inside the
        // injected wash escapes must never be re-matched and wrapped —
        // stripping every escape from the output must give back exactly
        // the original plain text.
        let mut washy = highlight("sancdown", "sanctuary", NamedColor::Yellow);
        if let TriggerAction::Highlight { style } = &mut washy.actions[0] {
            style.wash = true;
        }
        let digits = highlight("numbers", r"\d+", NamedColor::Red);
        let s = store(vec![washy, digits]);
        let r = process(&s, b"sanctuary fades in 42 seconds", SESSION);
        let text = r.display.unwrap();
        assert_eq!(plain_text(text.as_bytes()), "sanctuary fades in 42 seconds");
        // And the digit highlight still landed on the real digits.
        assert!(text.contains("\x1b[31m42"));
    }

    #[test]
    fn overlapping_highlights_first_wins() {
        // Two patterns overlapping on the same text: the higher-priority
        // trigger keeps its span, the overlapping later span is dropped
        // rather than nested mid-escape.
        let mut a = highlight("phrase", "tells you", NamedColor::Cyan);
        a.priority = 10;
        let b = highlight("word", "you", NamedColor::Red);
        let s = store(vec![a, b]);
        let r = process(&s, b"Bob tells you 'hi'", SESSION);
        let text = r.display.unwrap();
        assert!(text.contains("\x1b[36mtells you\x1b[0m"));
        assert!(!text.contains("\x1b[31m"));
        assert_eq!(plain_text(text.as_bytes()), "Bob tells you 'hi'");
    }

    #[test]
    fn prompt_target_skipped_on_line_scope() {
        let s = store(vec![Trigger {
            target: TriggerTarget::Prompt,
            ..Trigger::new("p", "hp", TriggerAction::Gag)
        }]);
        // Line-scope pass MUST NOT fire a prompt-target trigger.
        let r = process_scoped(&s, b"100/100 hp", MatchScope::Line, SESSION);
        assert_eq!(r.display.as_deref(), Some("100/100 hp"));
    }

    #[test]
    fn prompt_target_fires_on_prompt_scope() {
        let s = store(vec![Trigger {
            target: TriggerTarget::Prompt,
            ..Trigger::new("p", "hp", TriggerAction::Gag)
        }]);
        let r = process_scoped(&s, b"100/100 hp", MatchScope::Prompt, SESSION);
        assert!(r.display.is_none());
    }

    #[test]
    fn line_target_skipped_on_prompt_scope() {
        let s = store(vec![Trigger::new("l", "hp", TriggerAction::Gag)]);
        // Prompt-scope pass MUST NOT fire a line-target trigger.
        let r = process_scoped(&s, b"100/100 hp", MatchScope::Prompt, SESSION);
        assert_eq!(r.display.as_deref(), Some("100/100 hp"));
    }

    /// A Room trigger coloring the whole line yellow, at `priority`.
    fn room_yellow(priority: i32) -> Trigger {
        let mut t = highlight("room", "^.+$", NamedColor::Yellow);
        t.target = TriggerTarget::Room;
        t.priority = priority;
        t
    }

    #[test]
    fn a_room_trigger_fires_only_on_a_room_line() {
        let s = store(vec![room_yellow(4)]);
        let line = b"     A black-steel helm is here, gleaming darkly.";
        assert_eq!(
            process_scoped(&s, line, MatchScope::Room, SESSION)
                .display
                .as_deref(),
            Some("\x1b[33m     A black-steel helm is here, gleaming darkly.\x1b[0m")
        );
        for scope in [MatchScope::Line, MatchScope::Prompt] {
            let r = process_scoped(&s, line, scope, SESSION);
            assert_eq!(
                r.display.as_deref(),
                Some("     A black-steel helm is here, gleaming darkly."),
                "{scope:?}"
            );
        }
    }

    #[test]
    fn a_room_line_runs_line_and_room_triggers_in_one_pass() {
        // A Line trigger on a name, at a higher priority, keeps its span,
        // and the Room trigger's whole line span overlaps it, so it drops,
        // the way two Line triggers resolve.
        let mut name = highlight("name", "Tolliver", NamedColor::Cyan);
        name.priority = 5;
        let mut sends = highlight("greet", "^Tolliver is resting here\\.$", NamedColor::Red);
        sends.actions = vec![TriggerAction::Send {
            template: "wave".into(),
        }];
        let s = store(vec![room_yellow(4), name, sends]);
        let r = process_scoped(&s, b"Tolliver is resting here.", MatchScope::Room, SESSION);
        assert_eq!(
            r.display.as_deref(),
            Some("\x1b[36mTolliver\x1b[0m is resting here.")
        );
        assert_eq!(r.sends, vec!["wave".to_string()]);
        let names: Vec<String> =
            matching(&s, "Tolliver is resting here.", MatchScope::Room, SESSION)
                .into_iter()
                .map(|t| t.name.clone())
                .collect();
        assert_eq!(names, ["name", "room", "greet"]);
        let line_only: Vec<String> =
            matching(&s, "Tolliver is resting here.", MatchScope::Line, SESSION)
                .into_iter()
                .map(|t| t.name.clone())
                .collect();
        assert_eq!(line_only, ["name", "greet"]);
    }

    /// A base color trigger: `fg` fills what the line leaves in the
    /// default color.
    fn base(name: &str, pattern: &str, fg: NamedColor, priority: i32) -> Trigger {
        let mut t = highlight(name, pattern, fg);
        t.priority = priority;
        t.actions = vec![TriggerAction::Highlight {
            style: HighlightStyle {
                fg: Some(fg),
                base: true,
                ..Default::default()
            },
        }];
        t
    }

    #[test]
    fn a_target_line_runs_line_room_and_target_triggers_in_one_pass() {
        // The target color sits above the room color, so its base wins
        // on the line of your target, and the room color holds on every
        // other room line.
        let mut target = base("target", "^.+$", NamedColor::BrightRed, 5);
        target.target = TriggerTarget::RoomTarget;
        let mut room = base("room", "^.+$", NamedColor::Yellow, 4);
        room.target = TriggerTarget::Room;
        let mut send = highlight("greet", "werebeast", NamedColor::Cyan);
        send.priority = 3;
        send.actions = vec![TriggerAction::Send {
            template: "nod".into(),
        }];
        let s = store(vec![room, target, send]);
        let text = "A young werebeast stands here, leaning on his spear.";
        let shown = |scope| process_scoped(&s, text.as_bytes(), scope, SESSION).display;
        assert_eq!(
            shown(MatchScope::RoomTarget).as_deref(),
            Some(format!("\x1b[91m{text}\x1b[0m").as_str())
        );
        assert_eq!(
            shown(MatchScope::Room).as_deref(),
            Some(format!("\x1b[33m{text}\x1b[0m").as_str())
        );
        for scope in [MatchScope::Line, MatchScope::Prompt] {
            assert_eq!(shown(scope).as_deref(), Some(text), "{scope:?}");
        }
        let names = |scope| -> Vec<String> {
            matching(&s, text, scope, SESSION)
                .into_iter()
                .map(|t| t.name.clone())
                .collect()
        };
        assert_eq!(names(MatchScope::RoomTarget), ["target", "room", "greet"]);
        assert_eq!(names(MatchScope::Room), ["room", "greet"]);
        assert_eq!(names(MatchScope::Line), ["greet"]);
        assert_eq!(names(MatchScope::Prompt), Vec::<String>::new());
    }

    #[test]
    fn a_your_target_trigger_round_trips_as_room_target() {
        let mut t = highlight("target", "^.+$", NamedColor::BrightRed);
        t.target = TriggerTarget::RoomTarget;
        let json = store(vec![t]).export_json().unwrap();
        assert!(json.contains("\"target\": \"room_target\""), "{json}");
        let mut back = TriggerStore::new();
        back.import_json(&json).unwrap();
        assert_eq!(
            back.get("target").unwrap().target,
            TriggerTarget::RoomTarget
        );
        assert!(TriggerTarget::RoomTarget.is_room());
        assert!(TriggerTarget::Room.is_room());
        assert!(!TriggerTarget::Line.is_room());
        assert!(!TriggerTarget::Prompt.is_room());
    }

    #[test]
    fn a_base_color_keeps_the_codes_the_game_sent() {
        // do_exits with a trap seen on a closed door, the + in `! bold red
        // and the game's reset after it.
        let s = store(vec![base("exits", r"^\[Exits:", NamedColor::Green, 6)]);
        let line = "[Exits: up (\x1b[0;1;31m+\x1b[0;0mdown)]";
        assert_eq!(
            process(&s, line.as_bytes(), SESSION).display.as_deref(),
            Some("\x1b[32m[Exits: up (\x1b[0;1;31m+\x1b[0;0m\x1b[32mdown)]\x1b[0m")
        );
        // A line with no codes takes the color whole.
        assert_eq!(
            process(&s, b"[Exits: south]", SESSION).display.as_deref(),
            Some("\x1b[32m[Exits: south]\x1b[0m")
        );
        // A line it does not match is left as sent.
        assert_eq!(
            process(&s, b"Obvious exits:", SESSION).display.as_deref(),
            Some("Obvious exits:")
        );
    }

    #[test]
    fn a_base_color_comes_back_after_each_reset_and_never_over_a_set_color() {
        let s = store(vec![base("room", "^.+$", NamedColor::Yellow, 4)]);
        // `[AFK] ` in `1 red, then a bare reset, a 39 and a reset inside a
        // longer sequence. A 256 color and a background with a 0 in its
        // arguments set nothing back.
        let line =
            "[\x1b[0;31mAFK\x1b[0;0m] a\x1b[mb\x1b[1;39mc\x1b[38;5;208md\x1b[48;5;0me\x1b[0;1mf";
        assert_eq!(
            process(&s, line.as_bytes(), SESSION).display.as_deref(),
            Some(
                "\x1b[33m[\x1b[0;31mAFK\x1b[0;0m\x1b[33m] a\x1b[m\x1b[33mb\x1b[1;39m\x1b[33mc\
                 \x1b[38;5;208md\x1b[48;5;0me\x1b[0;1m\x1b[33mf\x1b[0m"
            )
        );
    }

    #[test]
    fn a_span_draws_over_a_base_color_and_the_base_comes_back_after_it() {
        // A name highlight at a higher priority keeps its span, and the
        // whole line base color never drops for overlapping it.
        let mut name = highlight("name", "Tolliver", NamedColor::Cyan);
        name.priority = 5;
        let s = store(vec![base("room", "^.+$", NamedColor::Yellow, 4), name]);
        assert_eq!(
            process(&s, b"Tolliver is resting here.", SESSION)
                .display
                .as_deref(),
            Some("\x1b[33m\x1b[36mTolliver\x1b[0m\x1b[33m is resting here.\x1b[0m")
        );
    }

    #[test]
    fn a_base_color_fills_around_a_replace_and_its_own_codes() {
        let mut label = highlight("potion", "a bubbly brown potion", NamedColor::Red);
        label.actions = vec![TriggerAction::Replace {
            template: "a bubbly brown potion \x1b[38;5;248m(cure serious)\x1b[0m".into(),
        }];
        let s = store(vec![base("room", "^.+$", NamedColor::Yellow, 4), label]);
        assert_eq!(
            process(&s, b"a bubbly brown potion", SESSION)
                .display
                .as_deref(),
            Some(
                "\x1b[33ma bubbly brown potion \x1b[38;5;248m(cure serious)\x1b[0m\x1b[33m\x1b[0m"
            )
        );
    }

    #[test]
    fn the_first_base_color_in_priority_order_wins() {
        let s = store(vec![
            base("low", "^.+$", NamedColor::Yellow, 4),
            base("high", "^.+$", NamedColor::Green, 6),
        ]);
        assert_eq!(
            process(&s, b"x", SESSION).display.as_deref(),
            Some("\x1b[32mx\x1b[0m")
        );
    }

    #[test]
    fn a_base_color_round_trips_and_a_plain_style_writes_no_flag() {
        let s = store(vec![
            base("room", "^.+$", NamedColor::Yellow, 4),
            highlight("plain", "x", NamedColor::Red),
        ]);
        let json = s.export_json().unwrap();
        assert_eq!(json.matches("\"base\": true").count(), 1, "{json}");
        let mut back = TriggerStore::new();
        back.import_json(&json).unwrap();
        assert_eq!(back.list(), s.list());
    }

    #[test]
    fn a_room_target_round_trips_and_older_targets_stay_as_they_were() {
        let s = store(vec![room_yellow(4)]);
        let json = s.export_json().unwrap();
        assert!(json.contains("\"target\": \"room\""), "{json}");
        let mut back = TriggerStore::new();
        back.import_json(&json).unwrap();
        assert_eq!(back.get("room").unwrap().target, TriggerTarget::Room);
        let line = store(vec![highlight("line", "x", NamedColor::Red)])
            .export_json()
            .unwrap();
        assert!(!line.contains("\"target\""), "{line}");
    }

    const VELLUM: readable::Rgb = (0xf7, 0xf4, 0xee);
    const NORD: readable::Rgb = (0x2e, 0x34, 0x40);
    const WEATHER: readable::Rgb = (0x8f, 0xa7, 0xd9);

    /// A weather trigger that paints the line #8fa7d9, the way a
    /// `{#8fa7d9}$0{reset}` Replace template saves.
    fn weather_store() -> TriggerStore {
        store(vec![Trigger::new(
            "weather",
            "^It starts to rain\\.$",
            TriggerAction::Replace {
                template: "\x1b[38;2;143;167;217m$0\x1b[0m".into(),
            },
        )])
    }

    fn on_ground(s: &TriggerStore, line: &[u8], ground: Option<readable::Rgb>) -> String {
        let plain = plain_text(line);
        process_on_ground(s, line, &plain, MatchScope::Line, ground, None, SESSION)
            .display
            .unwrap()
    }

    #[test]
    fn a_true_color_highlight_reads_on_a_light_ground() {
        let s = weather_store();
        let (r, g, b) = readable::lift_to_contrast(WEATHER, VELLUM);
        assert_ne!((r, g, b), WEATHER);
        assert!(readable::contrast((r, g, b), VELLUM) >= readable::READABLE_CONTRAST);
        assert_eq!(
            on_ground(&s, b"It starts to rain.", Some(VELLUM)),
            format!("\x1b[38;2;{r};{g};{b}mIt starts to rain.\x1b[0m")
        );
    }

    #[test]
    fn a_true_color_highlight_that_reads_stays_on_a_dark_ground() {
        let s = weather_store();
        assert_eq!(
            on_ground(&s, b"It starts to rain.", Some(NORD)),
            "\x1b[38;2;143;167;217mIt starts to rain.\x1b[0m"
        );
    }

    #[test]
    fn no_ground_leaves_the_trigger_color_as_set() {
        let s = weather_store();
        let set = "\x1b[38;2;143;167;217mIt starts to rain.\x1b[0m";
        assert_eq!(on_ground(&s, b"It starts to rain.", None), set);
        assert_eq!(
            process(&s, b"It starts to rain.", SESSION)
                .display
                .as_deref(),
            Some(set)
        );
    }

    #[test]
    fn game_true_color_passes_through_untouched() {
        // The game paints its own line #8fa7d9. No trigger matches it, so
        // it keeps its bytes even on a ground it fades on.
        let s = weather_store();
        let line = b"\x1b[38;2;143;167;217mThe clouds disappear.\x1b[0m";
        assert_eq!(
            on_ground(&s, line, Some(VELLUM)).as_bytes(),
            line.as_slice()
        );
        // Nor does an empty store touch it.
        let empty = TriggerStore::new();
        assert_eq!(
            on_ground(&empty, line, Some(VELLUM)).as_bytes(),
            line.as_slice()
        );
    }

    #[test]
    fn palette_highlights_and_washes_stay_on_a_light_ground() {
        // A named highlight and a wash draw in the theme's own colors, which
        // the theme keeps readable, so the ground changes neither.
        let mut washed = highlight("w", "storm", NamedColor::BrightCyan);
        if let TriggerAction::Highlight { style } = &mut washed.actions[0] {
            style.wash = true;
        }
        let s = store(vec![washed]);
        let plain = process(&s, b"The snowstorm becomes a blizzard.", SESSION)
            .display
            .unwrap();
        assert_eq!(
            on_ground(&s, b"The snowstorm becomes a blizzard.", Some(VELLUM)),
            plain
        );
    }

    /// The name of room 5279 as `do_look` sends it inside, the 256 color
    /// 255 tint ahead of the grey of the name (fixtures/room-colors). 255
    /// reads about 1.1 to 1 on Vellum, so a lift that reached the game's
    /// codes would change it.
    const BANK: &str = "\x1b[38;5;255m\x1b[0;1;30mThe Bank of Aabahran\x1b[0;0m\x1b[0;0m";

    #[test]
    fn a_fixed_color_drawn_in_place_reads_on_a_light_ground() {
        // A span in the weather blue over `Bank`. The game's grey stops at
        // the span and comes back after it, and its tint stays as sent.
        let plain = plain_text(BANK.as_bytes());
        let at = plain.find("Bank").unwrap();
        let spans = vec![(at, at + 4, "\x1b[38;2;143;167;217m".to_string())];
        let shown = |(r, g, b): readable::Rgb| {
            format!(
                "\x1b[38;5;255m\x1b[0;1;30mThe \x1b[0;38;2;{r};{g};{b}mBank\x1b[0;1;30m \
                 of Aabahran\x1b[0;0m\x1b[0;0m"
            )
        };
        let lifted = readable::lift_to_contrast(WEATHER, VELLUM);
        assert_ne!(lifted, WEATHER);
        let draw = |ground| draw_in_place(BANK.as_bytes(), &plain, spans.clone(), ground);
        assert_eq!(draw(None), shown(WEATHER));
        assert_eq!(draw(Some(NORD)), shown(WEATHER));
        assert_eq!(draw(Some(VELLUM)), shown(lifted));

        // A line whose bytes do not spell its plain text is rebuilt, and
        // its span reads on the ground too.
        let (r, g, b) = lifted;
        assert_eq!(
            draw_in_place(
                b"\x1b[0;32mMaren\x1b[0;0m",
                "Tolliver",
                vec![(0, 8, "\x1b[38;2;143;167;217m".to_string())],
                Some(VELLUM),
            ),
            format!("\x1b[38;2;{r};{g};{b}mTolliver\x1b[0m")
        );
    }

    #[test]
    fn the_game_colors_around_a_highlight_in_place_stay_on_a_light_ground() {
        let s = store(vec![highlight("bank", "Bank", NamedColor::Cyan)]);
        let shown = on_ground(&s, BANK.as_bytes(), Some(VELLUM));
        assert_eq!(
            shown,
            "\x1b[38;5;255m\x1b[0;1;30mThe \x1b[0;36mBank\x1b[0;1;30m of Aabahran\x1b[0;0m\x1b[0;0m"
        );
        assert_eq!(Some(shown), process(&s, BANK.as_bytes(), SESSION).display);
    }

    /// The Room, time, and weather colors preset as presets.ts makes it,
    /// from fixtures/room-colors/preset.json.
    fn room_preset() -> TriggerStore {
        #[derive(serde::Deserialize)]
        struct PresetFile {
            triggers: Vec<Trigger>,
        }
        let file: PresetFile =
            serde_json::from_str(include_str!("../../../../fixtures/room-colors/preset.json"))
                .unwrap();
        store(file.triggers)
    }

    /// The lines of fixtures/room-colors/lines.json the preset's `trigger`
    /// colors, or the near misses no trigger of it touches when `trigger`
    /// is None.
    fn room_lines(trigger: Option<&str>) -> Vec<String> {
        let file: serde_json::Value =
            serde_json::from_str(include_str!("../../../../fixtures/room-colors/lines.json"))
                .unwrap();
        file["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| match trigger {
                None => l.get("trigger").is_none(),
                Some(name) => l["trigger"] == name,
            })
            .map(|l| l["line"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn each_weather_line_draws_in_the_weather_blue_and_reads_on_every_ground() {
        let s = room_preset();
        let weather = room_lines(Some("weather.change"));
        // Fourteen from sky_event_text and ten from weather_affect_room.
        assert_eq!(weather.len(), 24);
        let grounds: serde_json::Value =
            serde_json::from_str(include_str!("../../../../fixtures/readable/grounds.json"))
                .unwrap();
        let grounds: Vec<(String, readable::Rgb)> = grounds["grounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| {
                let hex = g["background"].as_str().unwrap();
                let theme = g["theme"].as_str().unwrap().to_string();
                (theme, readable::parse_hex(hex).unwrap())
            })
            .collect();
        let lifted = readable::lift_to_contrast(WEATHER, VELLUM);
        assert_ne!(lifted, WEATHER);
        assert!(readable::contrast(lifted, VELLUM) >= readable::READABLE_CONTRAST);
        for line in &weather {
            let plain = plain_text(line.as_bytes());
            let drawn = |(r, g, b): readable::Rgb| format!("\x1b[38;2;{r};{g};{b}m{plain}\x1b[0m");
            // The whole line in the blue, in place of the bold white the
            // game sends with a change in the sky.
            assert_eq!(on_ground(&s, line.as_bytes(), None), drawn(WEATHER));
            assert_eq!(on_ground(&s, line.as_bytes(), Some(NORD)), drawn(WEATHER));
            assert_eq!(on_ground(&s, line.as_bytes(), Some(VELLUM)), drawn(lifted));
            // On every built in ground it reads, and it changes on the
            // light ones alone.
            let mut changed = Vec::new();
            for (theme, ground) in &grounds {
                let want = readable::lift_to_contrast(WEATHER, *ground);
                assert!(readable::contrast(want, *ground) >= readable::READABLE_CONTRAST);
                assert_eq!(
                    on_ground(&s, line.as_bytes(), Some(*ground)),
                    drawn(want),
                    "{plain} on {theme}"
                );
                if want != WEATHER {
                    changed.push(theme.as_str());
                }
            }
            assert_eq!(
                changed,
                [
                    "rubric",
                    "solarized-light",
                    "high-contrast-light",
                    "melange-light"
                ],
                "{plain}"
            );
        }
    }

    #[test]
    fn a_near_miss_of_the_room_preset_keeps_the_bytes_the_game_sent() {
        // A say or a tell that quotes a weather line, the weather report you
        // ask for, and the lines around a change in the weather among them.
        let s = room_preset();
        let misses = room_lines(None);
        assert!(misses.len() > 20, "{}", misses.len());
        assert!(misses.iter().any(|l| l.contains("It starts to rain.")));
        for line in &misses {
            let plain = plain_text(line.as_bytes());
            assert!(
                matching(&s, &plain, MatchScope::Line, SESSION).is_empty(),
                "{plain}"
            );
            assert_eq!(on_ground(&s, line.as_bytes(), Some(VELLUM)), *line);
        }
    }

    const RUBRIC: readable::Rgb = (0xf0, 0xe5, 0xcf);

    /// `line` through `s` with the game ground at `game` and no ground for
    /// trigger colors.
    fn on_game_ground(s: &TriggerStore, line: &[u8], game: Option<readable::Rgb>) -> String {
        let plain = plain_text(line);
        process_on_ground(s, line, &plain, MatchScope::Line, None, game, SESSION)
            .display
            .unwrap()
    }

    #[test]
    fn the_game_256_colors_lift_on_every_path_that_keeps_them() {
        // The white 255 tint ahead of the bank's name reads at Lc 0 on
        // Rubric's parchment.
        let tint = readable::lift_game_sgr(b"\x1b[38;5;255m", RUBRIC);
        let tint = std::str::from_utf8(&tint).unwrap();
        assert!(tint.starts_with("\x1b[38;2;"), "{tint:?}");
        let lifted = BANK.replacen("\x1b[38;5;255m", tint, 1);
        let in_place = store(vec![highlight("bank", "Bank", NamedColor::Cyan)]);
        let cases = [
            // No trigger runs.
            (TriggerStore::new(), lifted.clone()),
            // A trigger runs, and none matches.
            (weather_store(), lifted.clone()),
            // A highlight draws over the name in place, and the game's
            // codes around it keep the lift.
            (
                in_place,
                format!("{tint}\x1b[0;1;30mThe \x1b[0;36mBank\x1b[0;1;30m of Aabahran\x1b[0;0m\x1b[0;0m"),
            ),
        ];
        for (s, want) in &cases {
            assert_eq!(on_game_ground(s, BANK.as_bytes(), Some(RUBRIC)), *want);
            // With no game ground the game's colors stay as sent.
            let as_sent = on_game_ground(s, BANK.as_bytes(), None);
            assert_eq!(as_sent, want.replacen(tint, "\x1b[38;5;255m", 1));
        }
    }

    #[test]
    fn a_rebuilt_line_keeps_the_colors_its_trigger_paints() {
        // A Replace rebuilds the line from its plain text, so none of the
        // game's codes are left to lift, and the game ground leaves the
        // white 255 the trigger paints as set.
        let s = store(vec![Trigger::new(
            "weather",
            "^It starts to rain\\.$",
            TriggerAction::Replace {
                template: "\x1b[38;5;255m$0\x1b[0m".into(),
            },
        )]);
        let line = b"\x1b[0;1;37mIt starts to rain.\x1b[0;0m";
        assert_eq!(
            on_game_ground(&s, line, Some(RUBRIC)),
            "\x1b[38;5;255mIt starts to rain.\x1b[0m"
        );
    }
}
