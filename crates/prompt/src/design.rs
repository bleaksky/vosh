//! The prompt template grammar, version 2.
//!
//! A template is text with codes in it. The tokenizer keeps every form the
//! first grammar accepted and adds the forms of the prompt editor.
//!
//! ```text
//! %%                       a literal percent sign
//! %name  %{name}           a value, `name` is [A-Za-z0-9_], lowercased
//! %pct_name                the value as a percent of its max
//! %{name:pct:game}         the percent cut to a whole number, as the game
//!                          works it out
//! %name_bar[:W[:C]]        a bar W cells wide (default 10, 1 to 80) in color C
//! %bar_name[:W[:C]]        the same bar
//! %maxname                 the max, a field of its own
//! %c_<spec> %{c:<spec>}    foreground: a theme color name, a 256 index,
//!                          #rrggbb, r,g,b, `default`, `reset`, or a field
//!                          name to color by how full it is
//! %{c:hp:game}             foreground by the game's own %h bands
//! %{c:hp:steps}            foreground in eleven steps from red to green,
//!                          one for each tenth
//! %bg_<spec> %{bg:<spec>}  background, same specs
//! %{ul:<spec>}             the underline's color, same specs. Braced only,
//!                          so a value named `ul_...` stays a value
//! %s_<style> %{s:<style>}  bold dim italic underline inverse strike blink
//!                          off reset, and the underline kinds double curly
//!                          dotted dashed
//! %{field:format:args}     a value in a format (see [`Format`])
//! %{field:param:format}    for the fields that take a parameter, `aff`,
//!                          `member_*`, `queue` and `gmcp`
//! %{if:x} %{ifnot:x}       draw what follows up to %{end} only when x has a
//! %{end}                   value (or is hidden), or only when it has none
//! %nl %{nl}                a line break
//! %{right}                 push what follows on its row to the right edge.
//!                          Braced only, so a value named `right` stays one
//! %{raw}                   the game's own prompt, colors kept
//! ```
//!
//! A `%` followed by anything else stays literal, so `%)h` prints `%)h`.
//! A code the grammar does not know becomes an [`TokenKind::Unknown`] token
//! and prints as written, so a typo stays visible.
//!
//! [`Template::parse`] also groups tokens into pieces, the parts the editor
//! shows. A piece is a run of color and style codes followed by one value
//! or one run of text. Two runs fold into one piece. `%X/%{maxX}` is a
//! current and max piece, and `%pct_X%%` a percent piece.

mod look;
mod pieces;
mod tokens;
mod write;

pub use pieces::{PieceKind, Template};
pub use tokens::{Code, ColorSpec, FieldRef, Format, Scale, TokenKind, UnderlineStyle};

pub(crate) use look::{bg, code, color, fg, restore, transition, underline_color, Item, Look, Own};
pub(crate) use pieces::Piece;
pub(crate) use tokens::{brace_char, parse_field, BarColor, Layer, Style, ValueRef};
pub(crate) use tokens::{BAR_DEFAULT_WIDTH, BAR_MAX_WIDTH};
pub(crate) use write::{runs_on, write_token, write_tokens};

#[cfg(test)]
mod tests;
