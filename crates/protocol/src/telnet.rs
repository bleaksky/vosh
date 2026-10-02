//! Telnet protocol parser and option negotiator.
//!
//! The [`parser::Parser`] is a streaming state machine. Feed it bytes from the
//! socket and it returns a sequence of [`parser::Event`] values. The
//! [`negotiator::Negotiator`] takes those events and produces response bytes
//! to send back. The session loop owns both and pipes data between them and
//! the socket.

pub mod codes;
pub mod negotiator;
pub mod parser;

pub use codes::{option, IAC};
pub use negotiator::{Negotiator, DEFAULT_MTTS_BITS, DEFAULT_TERMINAL_TYPE};
pub use parser::{Event, Parser};
