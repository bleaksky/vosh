//! A parsed template, its tokens grouped into the pieces the editor shows.

use std::collections::BTreeSet;
use std::ops::Range;

use serde::Serialize;

use super::tokens::{tokenize, Token, TokenKind};
use super::tokens::{FieldRef, Format, ValueRef};

/// What a piece holds after its leading codes. The card reads it by name,
/// as `cur_max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PieceKind {
    /// Codes with nothing after them, at the end of the template or before
    /// a line break or a condition.
    Codes,
    /// A run of text and `%%`.
    Text,
    /// One value.
    Value,
    /// `%X/%{maxX}`, a value, a slash and its own max.
    CurMax,
    /// `%pct_X%%`, a percent and its sign.
    Percent,
    Nl,
    /// `%{right}`, the push to the right edge.
    Right,
    Raw,
    If,
    IfNot,
    End,
    Unknown,
}

/// One part of a template as the editor shows it. `codes` and `content`
/// index [`Template::tokens`] and sit next to each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Byte range in the template, codes included.
    pub start: usize,
    pub end: usize,
    pub codes: Range<usize>,
    pub content: Range<usize>,
    pub kind: PieceKind,
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    source: String,
    tokens: Vec<Token>,
    pieces: Vec<Piece>,
}

impl Template {
    /// Parse a template. Parsing never fails. Anything malformed stays
    /// literal or becomes an unknown token that prints as written.
    pub fn parse(source: &str) -> Self {
        let tokens = tokenize(source);
        let pieces = group(&tokens);
        Self {
            source: source.to_string(),
            tokens,
            pieces,
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// The template has no source. Test only. The tests in `tests/` reach
    /// it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    /// The template text of one token, exactly as written.
    pub fn token_text(&self, index: usize) -> &str {
        let token = &self.tokens[index];
        &self.source[token.start..token.end]
    }

    /// The template text of one piece, codes included.
    pub fn piece_text(&self, index: usize) -> &str {
        let piece = &self.pieces[index];
        &self.source[piece.start..piece.end]
    }

    /// Every field the template reads, in values, conditions and colors
    /// by value. `%{raw}` reads the field `raw`.
    pub(crate) fn reads(&self) -> BTreeSet<FieldRef> {
        let mut out = BTreeSet::new();
        for token in &self.tokens {
            token.kind.each_read(&mut |field| {
                out.insert(field.clone());
            });
        }
        out
    }
}

/// True when `max` reads the max of `base`, in any spelling the first
/// grammar looked a max up by, or as `%{base:max}`.
///
/// `max_spellings` in the values module holds the same four spellings in
/// the order a lookup tries them. Here any one of them matches, so the
/// order changes nothing. The design module reads nothing from values, so
/// this list stays here in its own order.
fn is_max_of(base: &ValueRef, max: &ValueRef) -> bool {
    if base.format != Format::Value || base.field.param.is_some() || max.field.param.is_some() {
        return false;
    }
    let name = &base.field.name;
    match max.format {
        Format::Max => max.field.name == *name,
        Format::Value => {
            let m = &max.field.name;
            *m == format!("max{name}")
                || *m == format!("m{name}")
                || *m == format!("{name}_max")
                || *m == format!("max_{name}")
        }
        _ => false,
    }
}

/// Group tokens into pieces.
fn group(tokens: &[Token]) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut i = 0;
    let piece = |codes: Range<usize>, content: Range<usize>, kind: PieceKind| {
        let first = if codes.is_empty() {
            content.start
        } else {
            codes.start
        };
        let last = if content.is_empty() {
            codes.end
        } else {
            content.end
        };
        Piece {
            start: tokens[first].start,
            end: tokens[last - 1].end,
            codes,
            content,
            kind,
        }
    };
    while i < tokens.len() {
        let codes_start = i;
        while i < tokens.len() && matches!(tokens[i].kind, TokenKind::Code(_)) {
            i += 1;
        }
        let codes = codes_start..i;
        if i == tokens.len() {
            if !codes.is_empty() {
                pieces.push(piece(codes, i..i, PieceKind::Codes));
            }
            break;
        }
        let marker = match tokens[i].kind {
            TokenKind::If(_) => Some(PieceKind::If),
            TokenKind::IfNot(_) => Some(PieceKind::IfNot),
            TokenKind::End => Some(PieceKind::End),
            TokenKind::Nl => Some(PieceKind::Nl),
            TokenKind::Right => Some(PieceKind::Right),
            _ => None,
        };
        if let Some(kind) = marker {
            if !codes.is_empty() {
                pieces.push(piece(codes.clone(), codes.end..codes.end, PieceKind::Codes));
            }
            pieces.push(piece(i..i, i..i + 1, kind));
            i += 1;
            continue;
        }
        let (len, kind) = match &tokens[i].kind {
            TokenKind::Text(_) | TokenKind::Percent => {
                let run = tokens[i..]
                    .iter()
                    .take_while(|t| matches!(t.kind, TokenKind::Text(_) | TokenKind::Percent))
                    .count();
                (run, PieceKind::Text)
            }
            TokenKind::Value(base) => {
                let slash_max = match (tokens.get(i + 1), tokens.get(i + 2)) {
                    (
                        Some(Token {
                            kind: TokenKind::Text(slash),
                            ..
                        }),
                        Some(Token {
                            kind: TokenKind::Value(max),
                            ..
                        }),
                    ) => slash == "/" && is_max_of(base, max),
                    _ => false,
                };
                let percent = base.format == Format::Pct
                    && matches!(
                        tokens.get(i + 1),
                        Some(Token {
                            kind: TokenKind::Percent,
                            ..
                        })
                    );
                if slash_max {
                    (3, PieceKind::CurMax)
                } else if percent {
                    (2, PieceKind::Percent)
                } else {
                    (1, PieceKind::Value)
                }
            }
            TokenKind::Raw => (1, PieceKind::Raw),
            _ => (1, PieceKind::Unknown),
        };
        pieces.push(piece(codes, i..i + len, kind));
        i += len;
    }
    pieces
}
