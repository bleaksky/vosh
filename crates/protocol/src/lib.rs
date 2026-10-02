//! The wire, with no app state.
//!
//! [`telnet`] parses the telnet stream and answers its option negotiation.
//! [`gmcp`] reads and writes the GMCP messages that ride inside it.
//! [`ansi`] reads the escape sequences in the lines the game sends.

pub mod ansi;
pub mod gmcp;
pub mod telnet;
