// The daemon facade assembles responsibility-oriented sections in dependency order.
// Keeping the sections in one private namespace preserves the existing API while
// making ownership and review boundaries explicit.
#[path = "developer_backup.rs"]
pub mod developer_backup;
#[path = "firmware_bundle.rs"]
pub mod firmware_bundle;
#[path = "lan.rs"]
pub mod lan;

include!("lib_core/foundation.rs");
include!("lib_core/contracts.rs");
include!("lib_core/control_plane.rs");
include!("lib_core/http.rs");
include!("lib_core/wifi_thermal.rs");
include!("lib_core/firmware_update.rs");
include!("lib_core/serial.rs");
include!("lib_core/flash.rs");
include!("lib_core/discovery.rs");
include!("lib_core/espflash.rs");
include!("lib_core/events.rs");

#[cfg(test)]
#[path = "lib_core_tests.rs"]
mod tests;
