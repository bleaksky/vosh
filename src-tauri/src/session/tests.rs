//! The session's tests, one file per part of it. They sit inside
//! `session` so they can drive its private steps, and the glob import
//! hands each file those steps through `super`.

use super::*;

mod batch;
mod effects;
mod gmcp;
mod log_sink;
mod steps;
