//! The custom prompt engine.
//!
//! This crate holds every byte decision about the prompt Vosh draws, with
//! no Tauri, so each one is testable on its own. The session task feeds it
//! and emits what it returns.
//!
//! - [`engine`] is [`PromptEngine`], the front door the session keeps for
//!   the live profile. It follows Char.Prompt, reads the game's replies to
//!   `prompt` and `fprompt`, counts misses and reports the status.
//! - [`config`] is the `[prompt]` table a profile file saves, with
//!   Vosh's default design and the rule that a profile with no design of
//!   its own follows the game. It also holds Vosh's vitals text.
//! - [`design`] is the template language. It parses a design into tokens
//!   and the pieces the editor shows, writes tokens back as text, and
//!   holds the look algebra the editor uses to keep each piece's look.
//! - [`values`] holds the value catalog, the value formats, the GMCP
//!   snapshot, what the game hides and why, the resolver that answers the
//!   renderer, the samples and a preview's overrides.
//! - [`render`](mod@render) draws a design as ANSI text with a span per
//!   piece, and tracks the SGR state as it writes.
//! - [`capture`] recognizes your prompt in what the game sends, by your
//!   pattern or by Aabahran's shapes. It also reads the capture triggers
//!   older builds used, and builds a capture from a line another game
//!   prints.
//! - [`aabahran`] holds what Vosh knows about Aabahran alone, the PROMPT
//!   compiler with its codes, colors, lexing and shapes, the observer of
//!   the game's replies, and who the prompt is for.
//! - [`stage`] decides what Vosh writes to the terminal around your
//!   prompt, in one output per socket read, with the regions a later
//!   output replaces. It fills the candidates ring on each send and each
//!   GA or EOR.
//! - [`card`] builds what the prompt card on the page receives. It reports
//!   each field's state, says what each piece of a design is, makes the
//!   edits, reports what a capture compiles to, offers the presets, writes
//!   the sentences you read and groups the candidates.
//! - [`legacy`] rewrites your 0.7 vitals template in today's codes.
//! - `testkit`, behind the `testkit` feature, prints prompts the way the
//!   game does, plays a fake Aabahran for tests and scripted runs, and
//!   holds the designs and clocks many tests share.
//! - [`wrap`] is the word wrap both renderers share.
//!
//! Bare tags in the comments, such as (D10), section 3, decision 6 and
//! correction 27, point at the prompt build spec, which lives outside the
//! repo. A tag that names the refactor plan points at
//! `docs/refactor-plan.md`.

pub mod aabahran;
pub mod capture;
pub mod card;
pub mod config;
pub mod design;
pub mod engine;
pub mod legacy;
pub mod render;
pub mod stage;
#[cfg(feature = "testkit")]
pub mod testkit;
pub mod values;
pub mod wrap;

pub use config::{CaptureConfig, PromptConfig, PromptShow, DEFAULT_DESIGN, DEFAULT_VITALS_TEXT};
pub use design::{FieldRef, Template};
pub use engine::{GamePromptSeen, PromptEngine, Status};
pub use render::{render, render_str, RenderOptions, Rendered, Span};
pub use values::format::{Resolved, Value};
pub use values::{Capture, ClientValues, Values};

// The tests in `tests/` import these from the root.
#[cfg(feature = "testkit")]
pub use render::SpanColor;
#[cfg(feature = "testkit")]
pub use testkit::map_values::MapValues;
#[cfg(feature = "testkit")]
pub use values::Vars;
