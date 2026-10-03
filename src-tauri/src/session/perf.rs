//! Counters the session loop keeps on its hot path, rolled up once a second.

use std::time::Duration;

pub(super) const PERF_REPORT_INTERVAL: Duration = Duration::from_secs(1);

/// Hot-path performance counters owned by the single `io_loop` task.
/// Plain `u64` fields are fine because nothing else writes to them.
/// Rolled up once per second by `report_and_reset` and emitted as
/// one `tracing::debug!` line on the `vosh::perf` target. Silent
/// under default `RUST_LOG=info`; bring it back with
/// `RUST_LOG=info,vosh::perf=debug` when revisiting the save/IO
/// audit numbers, or `RUST_LOG=vosh::perf=debug` to see only the
/// per-second rollup.
///
/// Originally landed as Phase 1 instrumentation for the save/IO
/// performance audit, kept in the code at debug level so future
/// measurements do not need to re-instrument the hot path. The
/// per-line `Instant::now()` cost is single-digit ns on macOS so
/// the counters can stay live with no measurable overhead.
#[derive(Default)]
pub(super) struct PerfCounters {
    pub(super) socket_reads: u64,
    pub(super) bytes_in: u64,
    pub(super) lines_processed: u64,
    pub(super) trigger_lua_ns: u64,
    pub(super) mutex_wait_ns: u64,
    pub(super) mutex_acquires: u64,
    pub(super) log_append_ns: u64,
    pub(super) log_appends: u64,
    pub(super) scrollback_push_ns: u64,
    pub(super) scrollback_pushes: u64,
    pub(super) output_emits: u64,
    pub(super) output_emit_bytes: u64,
    pub(super) gmcp_packets: u64,
    /// Game ticks, from World.Time or a line that matches the Reset on
    /// pattern.
    pub(super) ticks: u64,
    pub(super) routed_emits: u64,
}

impl PerfCounters {
    /// Emit a single `debug!` line summarising the last second of work
    /// (or nothing at all if the session was idle) and zero the
    /// counters. Per-event averages are reported in microseconds so
    /// the user can eyeball lock contention without doing the math.
    pub(super) fn report_and_reset(&mut self) {
        let any_activity = self.socket_reads > 0
            || self.lines_processed > 0
            || self.gmcp_packets > 0
            || self.ticks > 0;
        if !any_activity {
            return;
        }
        let div_us = |total_ns: u64, n: u64| -> u64 { total_ns.checked_div(n).unwrap_or(0) / 1000 };
        let avg_trigger_us = div_us(self.trigger_lua_ns, self.lines_processed);
        let avg_lock_us = div_us(self.mutex_wait_ns, self.mutex_acquires);
        let avg_append_us = div_us(self.log_append_ns, self.log_appends);
        let avg_sb_us = div_us(self.scrollback_push_ns, self.scrollback_pushes);
        tracing::debug!(
            target: "vosh::perf",
            reads = self.socket_reads,
            bytes = self.bytes_in,
            lines = self.lines_processed,
            avg_trigger_us,
            avg_lock_us,
            lock_acq = self.mutex_acquires,
            avg_append_us,
            appends = self.log_appends,
            avg_sb_us,
            sb_pushes = self.scrollback_pushes,
            emits = self.output_emits,
            emit_bytes = self.output_emit_bytes,
            gmcp = self.gmcp_packets,
            ticks = self.ticks,
            routes = self.routed_emits,
            "perf 1s"
        );
        *self = Self::default();
    }
}
