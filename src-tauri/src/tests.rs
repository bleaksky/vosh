//! Tests that span the crate rather than one module.

mod broadcast;
mod config_golden;
#[cfg(native_surface)]
mod echo;
mod fake_mud;
mod ipc_contract;
#[cfg(native_surface)]
mod throughput;
mod upgrade_order;
mod wizard_roundtrip;
