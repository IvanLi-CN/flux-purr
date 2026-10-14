//! Compatibility re-export for the standalone PD HIL image.
//!
//! The executable remains an independent RAM image; only transport-neutral
//! wire helpers are shared through `flux-purr-pd-sink-core`.
#[allow(unused_imports)]
pub use flux_purr_pd_sink_core::*;
